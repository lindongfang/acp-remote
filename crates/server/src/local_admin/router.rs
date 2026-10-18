//! 方法路由：把一条已通过信封校验的管理请求分派到 §5 的方法实现
//! （`docs/LOCAL_ADMIN_PROTOCOL.md` §5、`docs/MODULE_ARCHITECTURE.md` §4.9、design.md 决策 6）。
//!
//! 边界纪律：
//!
//! - 业务方法只调用 `core::use_cases`，且恒以 `Actor::LocalCli` 调用（`require_local` 已就位）：
//!   server 侧不做第二套授权判定，也不直接查询 SQLite；
//! - `daemon.status`/`daemon.stop` 由组合根经注入的 [`DaemonControl`] 回答（server 不依赖 `app`）；
//! - `device.pair.*`/`node.pair.*` 的编排：领域值由注入的 `identity_auth::Authority`（经
//!   [`PairingSessions`]）产生，写集由 `core::use_cases` 单事务提交；内存态（已批准）只在提交成功后置位；
//! - `provider.configure` 的凭据值只经 `identity_auth::IdentityKeystore` 端口写入，值只以
//!   [`SecretBytes`] 形态流转（不进日志、不进错误、不进 `result`）；
//! - `audit.export` 直接用注入的 `Arc<dyn AuditStore>`（§5.1：不经业务用例）；
//! - 本模块不写审计：撤销类与 `provider.configure` 的审计由 core 的写集在同一事务里提交；
//!   组合根层面的拒绝连接审计由 WP3a 的 `AuditHook` 承担。§14.2 没有「方法失败」这一类类别，
//!   因此路由层失败只写结构化日志（method + 错误码，不含参数），不伪造审计事件。

use std::sync::Arc;

use acp_core::model::{
    Actor, Fingerprint, NodeKind, PairingId, PairingRecord, PairingState, PairingTarget,
    PeerIdentity, Timestamp,
};
use acp_core::ports::{AuditQuery, AuditStore, Clock, TrustRecordRef};
use acp_core::use_cases::UseCases;
use identity_auth::{IdentityKeystore, PairingDecision, PairingSpec};
use serde_json::Value;

use crate::local_admin::audit;
use crate::local_admin::daemon::DaemonControl;
use crate::local_admin::envelope::{AdminRequest, AdminResponse, JsonObject};
use crate::local_admin::error::{AdminError, LocalErrorCode};
use crate::local_admin::handler::LocalAdminHandler;
use crate::local_admin::method::Method;
use crate::local_admin::pairing::{self, PairingSessions};
use crate::local_admin::params;
use crate::local_admin::view::{self, object, text, timestamp};

/// [`LocalAdminRouter`] 的组合根注入口。
pub struct LocalAdminDeps {
    /// Daemon 生命周期与运行期状态（由 `app::daemon` 实现）。
    pub daemon: Arc<dyn DaemonControl>,
    /// 业务用例面（本地配置族、信任族）。
    pub core: Arc<UseCases>,
    /// Provider 凭据的平台安全存储端口（由组合根注入 `identity-keystore` 的实现）。
    pub keystore: Arc<dyn IdentityKeystore>,
    /// 审计仓储的**读取**端口（§5.1：`audit.export` 不经业务用例）。
    pub audit: Arc<dyn AuditStore>,
    /// 时间来源（core 不读系统时间：写集的时间戳由适配器经本端口取得后传入）。
    pub clock: Arc<dyn Clock>,
    /// 配对方法的注入口：共享的 `identity-auth` 状态机、本机 canonical origin（由组合根从配置注入）
    /// 与撤销后的连接关闭钩子（§5.3/§5.4）。
    pub pairing: Arc<PairingSessions>,
}

/// v1 管理方法的路由实现（替换 WP3a 的 `UnroutedAdminHandler`）。
pub struct LocalAdminRouter {
    deps: LocalAdminDeps,
}

impl LocalAdminRouter {
    /// 装配路由。
    pub fn new(deps: LocalAdminDeps) -> Self {
        Self { deps }
    }

    /// 处理一条请求并把错误收敛成一个响应（§4 规则 5：恰好一个响应，回带同一 `id`）。
    async fn dispatch(
        &self,
        method: Method,
        params: &JsonObject,
    ) -> Result<JsonObject, AdminError> {
        match method {
            Method::DaemonStatus => self.daemon_status(params).await,
            Method::DaemonStop => self.daemon_stop(params).await,
            Method::AgentList => self.agent_list(params).await,
            Method::AuditExport => self.audit_export(params).await,
            Method::DevicePairBegin => self.device_pair_begin(params).await,
            Method::DevicePairStatus => self.device_pair_status(params).await,
            Method::DevicePairConfirm => self.device_pair_confirm(params).await,
            Method::DevicePairReject => self.device_pair_reject(params).await,
            Method::DeviceList => self.device_list(params).await,
            Method::DeviceRevoke => self.device_revoke(params).await,
            Method::NodePairBegin => self.node_pair_begin(params).await,
            Method::NodePairStatus => self.node_pair_status(params).await,
            Method::NodePairConfirm => self.node_pair_confirm(params).await,
            Method::NodePairReject => self.node_pair_reject(params).await,
            Method::NodeList => self.node_list(params).await,
            Method::NodeRevoke => self.node_revoke(params).await,
            // §5.7：本方法只登记名字，`params`/`result` 与 `NODE_LINK_PROTOCOL.md` §12.3 的
            // `node.rotate-key.request`/`node.rotate-key.result` 同批定义；字段定义落地前调用恒回
            // `local.unsupported`，不自行填充参数形状。
            Method::NodeRotateKeyBegin => {
                Err(AdminError::unsupported_method(Method::NodeRotateKeyBegin))
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // daemon.*（§5.2）
    // -----------------------------------------------------------------------------------------

    async fn daemon_status(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        params::daemon_status(params)?;
        Ok(self.deps.daemon.status().await.to_json())
    }

    async fn daemon_stop(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        let grace_ms = params::daemon_stop(params)?;
        self.deps.daemon.stop(grace_ms).await?;
        // §5.2：`accepted` 恒为 `true`，失败必须走 `error`。
        Ok(object(vec![("accepted", Value::Bool(true))]))
    }

    // -----------------------------------------------------------------------------------------
    async fn agent_list(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        params::reject_unknown_fields(params, &[])?;
        let profiles = self
            .deps
            .core
            .agent_profiles(&Actor::LocalCli)
            .await
            .map_err(|error| params::map_port_error("agent.list", error))?;
        let entries = profiles
            .iter()
            .map(|profile| {
                Value::Object(object(vec![
                    ("agentId", text(profile.id().as_str())),
                    ("command", text(profile.command())),
                    (
                        "args",
                        Value::Array(profile.args().iter().map(text).collect()),
                    ),
                    ("default", Value::Bool(profile.is_default())),
                ]))
            })
            .collect();
        Ok(object(vec![("agents", Value::Array(entries))]))
    }

    // -----------------------------------------------------------------------------------------
    // 设备配对与信任（§5.3）
    // -----------------------------------------------------------------------------------------

    /// `device.pair.begin`（§5.3）：登记一次性配对并返回二维码 URL。
    ///
    /// 顺序刻意如此：先取全部「会失败但无副作用」的输入（origin、节点公钥、窗口），再写内存 secret，
    /// 最后提交持久配对行。`daemon.public_origin` 未配置时在任何写入之前回 `local.unavailable`
    /// （缺配置不是配对参数错误，也不得回落到 localhost）。
    async fn device_pair_begin(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "device.pair.begin";
        let begin = params::device_pair_begin(params)?;
        let origin = self.deps.pairing.canonical_origin(OPERATION)?;
        let host_public_key = self.deps.pairing.node_public_key(OPERATION).await?;
        let now = self.deps.clock.now();
        let expires_at = pairing::pairing_expires_at(&now, begin.expires_in_ms)?;
        let pairing_id = new_pairing_id()?;
        let draft = self
            .deps
            .pairing
            .authority()
            .begin_pairing(
                &pairing_id,
                &PairingSpec::Device {
                    canonical_origin: origin,
                },
                // §5.3 的 `device.pair.begin` 不携带展示名：对端名称由 claim 声明。
                None,
                &now,
                &expires_at,
            )
            .map_err(|error| params::map_pairing_error(OPERATION, error))?;
        let qr_token = draft.qr_token.as_ref().ok_or_else(|| {
            AdminError::new(
                LocalErrorCode::Internal,
                format!("{OPERATION}: the device pairing has no qr token"),
            )
        })?;
        let pairing_url =
            self.deps
                .pairing
                .device_pairing_url(OPERATION, qr_token, &host_public_key)?;
        // 提交失败时内存里会留下一个没有持久行的 secret：它无法被认领（认领判定依赖库里的配对行），
        // 并由过期扫描清除（`Authority::due_pairings`）。
        self.deps
            .core
            .create_pairing(&Actor::LocalCli, draft.record)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?;
        Ok(object(vec![
            ("pairingId", text(pairing_id.as_str())),
            ("pairingUrl", text(pairing_url)),
            // 网页入口的一次性配对码（`as_str()` 是规范 8 位大写形态，不落库、不进日志）。
            ("pairingCode", text(draft.code.as_str())),
            ("expiresAt", timestamp(&expires_at)),
        ]))
    }

    /// `device.pair.status`（§5.3）：claim 前只有 `state`/`expiresAt`，claim 后才有名称与指纹；
    /// **不返回短验证码，也不返回任何授权集合**（设备配对不授予授权）。
    async fn device_pair_status(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "device.pair.status";
        let pairing_id = params::pairing_id(params)?;
        let view = self
            .pairing_view(OPERATION, &pairing_id, PairingTarget::Device)
            .await?;
        Ok(object(vec![
            (
                "state",
                text(pairing::device_pairing_state_token(view.state)),
            ),
            ("displayName", optional_text(view.display_name.as_deref())),
            (
                "publicKeyFingerprint",
                optional_text(
                    view.public_key_fingerprint
                        .as_ref()
                        .map(Fingerprint::as_str),
                ),
            ),
            (
                "deviceId",
                match view.peer.as_ref() {
                    Some(PeerIdentity::Device(device)) => text(device.as_str()),
                    _ => Value::Null,
                },
            ),
            ("expiresAt", timestamp(&view.expires_at)),
        ]))
    }

    /// `device.pair.confirm`（§5.3）：创建 active 设备记录（设备方向不产生任何授权，因此确认不接受
    /// 授权集合参数）。
    async fn device_pair_confirm(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "device.pair.confirm";
        let confirm = params::device_pair_confirm(params)?;
        let record = self
            .require_pairing(OPERATION, &confirm.pairing_id, PairingTarget::Device)
            .await?;
        let now = self.deps.clock.now();
        params::require_pending_confirmation(OPERATION, &record, &now)?;
        let decision = PairingDecision::Approve;
        let settlement = self
            .deps
            .pairing
            .authority()
            .settle(&record, &decision, &now)
            .map_err(|error| params::map_pairing_error(OPERATION, error))?;
        let reference = self
            .deps
            .core
            .settle_pairing(&Actor::LocalCli, &confirm.pairing_id, settlement)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?;
        let TrustRecordRef::Device(device_id) = reference else {
            return Err(AdminError::new(
                LocalErrorCode::Internal,
                format!("{OPERATION}: settled pairing did not create a device record"),
            ));
        };
        // design D3：内存态（已批准）只在持久提交**成功之后**置位。返回 `false` 表示内存里已经没有
        // 该配对的 secret（例如刚被过期扫描清掉）：信任记录已持久化，方法照常成功，但记一条警告。
        if !self
            .deps
            .pairing
            .authority()
            .mark_pairing_approved(&confirm.pairing_id)
        {
            tracing::warn!(
                method = OPERATION,
                "pairing secret was gone before the approval was recorded"
            );
        }
        let confirmed_at = self.approved_at(OPERATION, &confirm.pairing_id).await?;
        Ok(object(vec![
            ("deviceId", text(device_id.as_str())),
            ("confirmedAt", timestamp(&confirmed_at)),
        ]))
    }

    /// `device.pair.reject`（§5.3）：终结配对，不创建任何信任。
    async fn device_pair_reject(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        self.pairing_reject_for(params, PairingTarget::Device, "device.pair.reject")
            .await
    }

    /// `device.list`（§5.3）：含 `pending` 与 `revoked` 的全部记录。
    async fn device_list(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        params::reject_unknown_fields(params, &[])?;
        let devices = self
            .deps
            .core
            .devices(&Actor::LocalCli)
            .await
            .map_err(|error| params::map_port_error("device.list", error))?;
        Ok(object(vec![(
            "devices",
            Value::Array(
                devices
                    .iter()
                    .map(|record| Value::Object(view::device(record)))
                    .collect(),
            ),
        )]))
    }

    /// `device.revoke`（§5.3）：提交后立即关闭该设备的 active connection 才返回。
    ///
    /// 先判定「存在且未撤销」（§7：重试得到 `local.not_found`；存储的幂等撤销会在调用后才体现）；
    /// 读-写之间的竞态无害：并发重复撤销最坏落到存储的幂等撤销。
    async fn device_revoke(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "device.revoke";
        let device_id = params::device_revoke(params)?;
        let actor = Actor::LocalCli;
        // `revoked_at` 与 `state = revoked` 由 `DeviceRecord::try_new` 绑定为等价（`core::model`）。
        let revocable = self
            .deps
            .core
            .device(&actor, &device_id)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?
            .is_some_and(|record| record.revoked_at().is_none());
        if !revocable {
            return Err(not_revocable(OPERATION, "device", device_id.as_str()));
        }
        self.deps
            .core
            .revoke_device(&actor, &device_id)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?;
        // 关闭发生在持久提交之后：即使关闭失败（由组合根记日志）也不撤销已提交的撤销。
        self.deps.pairing.close_device(&device_id).await;
        let record = self
            .deps
            .core
            .device(&actor, &device_id)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?;
        let revoked_at = record
            .as_ref()
            .and_then(|record| record.revoked_at())
            .cloned()
            .ok_or_else(|| not_readable(OPERATION, "device"))?;
        Ok(object(vec![
            ("deviceId", text(device_id.as_str())),
            ("revokedAt", timestamp(&revoked_at)),
        ]))
    }

    // -----------------------------------------------------------------------------------------
    // 节点配对与信任（§5.4）
    // -----------------------------------------------------------------------------------------

    /// `node.pair.begin`（§5.4）：只有 `mode = "owner"` 落地；`mode = "access"` 固定不支持。
    async fn node_pair_begin(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "node.pair.begin";
        let params::NodePairBegin::Owner = params::node_pair_begin(params)? else {
            // §5.4：`mode = "access"` 需要对 Owner 执行 HTTPS claim，本切片未落地。这里不构造任何
            // HTTP 调用、不写任何状态，也不另行填充参数形状（与 §5.7 的 `node.rotate-key.begin` 同款）。
            return Err(AdminError::new(
                LocalErrorCode::Unsupported,
                format!(
                    "{OPERATION}: mode `access` is not implemented yet (no outbound claim is issued)"
                ),
            ));
        };
        let display_name = self
            .deps
            .pairing
            .node_name()
            .map(str::to_owned)
            .unwrap_or_else(|| self.deps.pairing.authority().local_node().as_str()[..8].to_owned());
        let endpoint = self.deps.pairing.node_endpoint(OPERATION)?;
        let owner_public_key = self.deps.pairing.node_public_key(OPERATION).await?;
        let now = self.deps.clock.now();
        // §5.4 的 `node.pair.begin` 没有 `expiresInMs`：窗口固定 5 分钟（§5.3 同款上限）。
        let expires_at = pairing::pairing_expires_at(&now, None)?;
        let pairing_id = new_pairing_id()?;
        let draft = self
            .deps
            .pairing
            .authority()
            .begin_pairing(
                &pairing_id,
                &PairingSpec::Node {
                    endpoint,
                    // 本节点在该配对中的角色是 Owner（§5.4：对端是待确认的 Access Node）。
                    kind: NodeKind::Owner,
                },
                Some(&display_name),
                &now,
                &expires_at,
            )
            .map_err(|error| params::map_pairing_error(OPERATION, error))?;
        let pairing_url = self.deps.pairing.node_pairing_url(
            OPERATION,
            &pairing_id,
            &draft.secret,
            &expires_at,
            &owner_public_key,
        )?;
        self.deps
            .core
            .create_pairing(&Actor::LocalCli, draft.record)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?;
        Ok(object(vec![
            ("pairingId", text(pairing_id.as_str())),
            ("pairingUrl", text(pairing_url)),
            (
                "state",
                text(pairing::node_pairing_state_token(PairingState::Created)),
            ),
            ("expiresAt", timestamp(&expires_at)),
            // claim 之前不知道对端身份（本协议不派生短验证码）。
            ("peerNodeId", Value::Null),
            ("peerPublicKeyFingerprint", Value::Null),
        ]))
    }

    /// `node.pair.status`（§5.4）：本切片没有向 Owner 的远程刷新，`lastRefreshError` 恒为 `null`。
    async fn node_pair_status(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "node.pair.status";
        let pairing_id = params::pairing_id(params)?;
        let view = self
            .pairing_view(OPERATION, &pairing_id, PairingTarget::Node)
            .await?;
        Ok(object(vec![
            ("state", text(pairing::node_pairing_state_token(view.state))),
            (
                "peerNodeId",
                match view.peer.as_ref() {
                    Some(PeerIdentity::Node(node)) => text(node.as_str()),
                    _ => Value::Null,
                },
            ),
            (
                "peerPublicKeyFingerprint",
                optional_text(
                    view.public_key_fingerprint
                        .as_ref()
                        .map(Fingerprint::as_str),
                ),
            ),
            ("expiresAt", timestamp(&view.expires_at)),
            ("lastRefreshError", Value::Null),
        ]))
    }

    /// `node.pair.confirm`（§5.4）：创建信任记录（配对即完全信任，不携带授权集合）。
    ///
    /// 信任行提交成功后关闭该节点的现有活动连接，强制重新握手。
    ///
    /// `node.revoke` 与节点方向的 `node.pair.confirm` 都是**提交后关连接**，但两条路径的**语义与关闭码
    /// 不同**：撤销推 `node.trust.revoked` 并以 `4410` 关闭并停止重连；重新配对**不**推消息、以 `1000`
    /// （正常关闭）+ close reason 关闭，要求对端重连以重取 catalog（`NODE_LINK_PROTOCOL.md`
    /// §8.2/§14.2/§15）。
    ///
    async fn node_pair_confirm(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "node.pair.confirm";
        let confirm = params::node_pair_confirm(params)?;
        let record = self
            .require_pairing(OPERATION, &confirm.pairing_id, PairingTarget::Node)
            .await?;
        let now = self.deps.clock.now();
        params::require_pending_confirmation(OPERATION, &record, &now)?;
        let decision = PairingDecision::Approve;
        let settlement = self
            .deps
            .pairing
            .authority()
            .settle(&record, &decision, &now)
            .map_err(|error| params::map_pairing_error(OPERATION, error))?;
        let reference = self
            .deps
            .core
            .settle_pairing(&Actor::LocalCli, &confirm.pairing_id, settlement)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?;
        let TrustRecordRef::Node(node_id) = reference else {
            return Err(AdminError::new(
                LocalErrorCode::Internal,
                format!("{OPERATION}: settled pairing did not create a node record"),
            ));
        };
        if !self
            .deps
            .pairing
            .authority()
            .mark_pairing_approved(&confirm.pairing_id)
        {
            tracing::warn!(
                method = OPERATION,
                "pairing secret was gone before the approval was recorded"
            );
        }
        let confirmed_at = self.approved_at(OPERATION, &confirm.pairing_id).await?;
        // 提交成功后让该节点重新握手：配对的含义已变化（配对即完全信任），旧连接必须在新口径下重算。
        // 与 `node.revoke` 不同：这里不推 `node.trust.revoked`、也不以 4410 关闭，而是正常关闭让对端重连。
        self.deps.pairing.close_node_after_reauth(&node_id).await;
        Ok(object(vec![
            ("nodeId", text(node_id.as_str())),
            ("confirmedAt", timestamp(&confirmed_at)),
        ]))
    }

    /// `node.pair.reject`（§5.4）：终结配对，不创建任何信任。
    async fn node_pair_reject(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        self.pairing_reject_for(params, PairingTarget::Node, "node.pair.reject")
            .await
    }

    /// `node.list`（§5.4）。
    async fn node_list(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        params::reject_unknown_fields(params, &[])?;
        let nodes = self
            .deps
            .core
            .nodes(&Actor::LocalCli)
            .await
            .map_err(|error| params::map_port_error("node.list", error))?;
        Ok(object(vec![(
            "nodes",
            Value::Array(
                nodes
                    .iter()
                    .map(|record| Value::Object(view::node(record)))
                    .collect(),
            ),
        )]))
    }

    /// `node.revoke`（§5.4）：提交后关闭 active connection 并停止本地重连才返回。
    ///
    /// 先判定「存在且未撤销」（§7：重试得到 `local.not_found`；存储的幂等撤销会在调用后才体现）；
    /// 读-写之间的竞态无害：并发重复撤销最坏落到存储的幂等撤销。
    async fn node_revoke(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "node.revoke";
        let node_id = params::node_revoke(params)?;
        let actor = Actor::LocalCli;
        // §11.6：同一事务令两种角色行一起进入 `revoked`，因此「可撤销」= 行存在且没有任何一行
        // 已带撤销时间（`revoked_at` 与 `state = revoked` 由 `NodeRecord::try_new` 绑定为等价）。
        let revocable = {
            let rows = self
                .deps
                .core
                .nodes_for(&actor, &node_id)
                .await
                .map_err(|error| params::map_port_error(OPERATION, error))?;
            !rows.is_empty() && rows.iter().all(|record| record.revoked_at().is_none())
        };
        if !revocable {
            return Err(not_revocable(OPERATION, "node", node_id.as_str()));
        }
        self.deps
            .core
            .revoke_node(&actor, &node_id)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?;
        self.deps.pairing.close_node(&node_id).await;
        // 同一事务里两种角色行都进入 `revoked`（§11.6）：读回时必须全部带撤销时间。
        let rows = self
            .deps
            .core
            .nodes_for(&actor, &node_id)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?;
        let mut revoked_at: Option<Timestamp> = None;
        for record in &rows {
            let at = record
                .revoked_at()
                .cloned()
                .ok_or_else(|| not_readable(OPERATION, "node"))?;
            if revoked_at.is_none() {
                revoked_at = Some(at);
            }
        }
        let revoked_at = revoked_at.ok_or_else(|| not_readable(OPERATION, "node"))?;
        Ok(object(vec![
            ("nodeId", text(node_id.as_str())),
            ("revokedAt", timestamp(&revoked_at)),
        ]))
    }

    // -----------------------------------------------------------------------------------------
    // 配对方法的公共编排（§5.3/§5.4）
    // -----------------------------------------------------------------------------------------

    /// `device.pair.reject` / `node.pair.reject` 的实现体（只有目标族与错误消息前缀不同）。
    async fn pairing_reject_for(
        &self,
        params: &JsonObject,
        target: PairingTarget,
        operation: &str,
    ) -> Result<JsonObject, AdminError> {
        let reject = params::pairing_reject(params)?;
        let record = self
            .require_pairing(operation, &reject.pairing_id, target)
            .await?;
        let now = self.deps.clock.now();
        params::require_pending_confirmation(operation, &record, &now)?;
        let decision = PairingDecision::Reject {
            reason: reject.reason,
        };
        let settlement = self
            .deps
            .pairing
            .authority()
            .settle(&record, &decision, &now)
            .map_err(|error| params::map_pairing_error(operation, error))?;
        // 拒绝路径不创建信任；返回值是对端引用，不在 `result` 里回显（§5.3/§5.4 的 `result` 是 `{}`）。
        self.deps
            .core
            .settle_pairing(&Actor::LocalCli, &reject.pairing_id, settlement)
            .await
            .map_err(|error| params::map_port_error(operation, error))?;
        Ok(object(vec![]))
    }

    /// 读取配对记录并校验目标族（`device.*` 只服务设备配对，`node.*` 只服务节点配对）。
    ///
    /// 未知 id 与「族不匹配」都回 `local.not_found`：对方法而言两者都是「本方法的目标域里没有它」。
    async fn require_pairing(
        &self,
        operation: &str,
        pairing_id: &PairingId,
        target: PairingTarget,
    ) -> Result<PairingRecord, AdminError> {
        self.deps
            .core
            .pairing(&Actor::LocalCli, pairing_id)
            .await
            .map_err(|error| params::map_port_error(operation, error))?
            .filter(|record| record.target() == target)
            .ok_or_else(|| {
                AdminError::new(
                    LocalErrorCode::NotFound,
                    format!("{operation}: no such pairing"),
                )
            })
    }

    /// 状态视图：记录 + 已认领的对端事实，交给状态机投影成 §5.3/§5.4 的可见字段。
    ///
    /// 本协议不派生短验证码（`pairing-model` D1/D2），因此这里不再取任何主机侧 SAS；投影出的字段
    /// 全部是状态、名称、指纹与非秘密标识，设备方向的授权集合也一并消失。
    async fn pairing_view(
        &self,
        operation: &str,
        pairing_id: &PairingId,
        target: PairingTarget,
    ) -> Result<identity_auth::PairingStatusView, AdminError> {
        let record = self.require_pairing(operation, pairing_id, target).await?;
        // 过期终结会把未认领的 `created` 行也写上 `claimed_at`（§7.3 的 CHECK 要求非
        // `created` 状态必须有该值），此时并不存在 peer 行；终态 `expired` 不需要 peer 详情。
        let peer =
            if record.claimed_at().is_some() && !matches!(record.state(), PairingState::Expired) {
                Some(
                    self.deps
                        .core
                        .pairing_peer(&Actor::LocalCli, pairing_id)
                        .await
                        .map_err(|error| params::map_port_error(operation, error))?
                        .ok_or_else(|| {
                            AdminError::new(
                                LocalErrorCode::Internal,
                                format!("{operation}: claimed pairing has no peer row"),
                            )
                        })?,
                )
            } else {
                None
            };
        let claimed = peer
            .as_ref()
            .map(|peer| pairing::claimed_pairing(&record, peer));
        Ok(self
            .deps
            .pairing
            .authority()
            .pairing_status(&record, claimed.as_ref()))
    }

    /// 读回持久化的 `approved_at`（`confirmedAt` 用持久值，不用本层时钟猜）。
    async fn approved_at(
        &self,
        operation: &str,
        pairing_id: &PairingId,
    ) -> Result<Timestamp, AdminError> {
        let record = self
            .deps
            .core
            .pairing(&Actor::LocalCli, pairing_id)
            .await
            .map_err(|error| params::map_port_error(operation, error))?;
        record
            .and_then(|record| record.approved_at().cloned())
            .ok_or_else(|| not_readable(operation, "approved pairing"))
    }

    // -----------------------------------------------------------------------------------------
    // 审计导出（§5.6）
    // -----------------------------------------------------------------------------------------

    async fn audit_export(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        let query = params::audit_export(params)?;
        let records = self
            .deps
            .audit
            .query(AuditQuery {
                since: query.since,
                until: query.until,
                actions: query.categories,
                actor: None,
                target: None,
                limit: None,
            })
            .await
            .map_err(|error| params::map_port_error("audit.export", error))?;
        let (record_count, sha256) =
            audit::write_export_file(&query.output_path, query.format, &records)?;
        Ok(object(vec![
            ("outputPath", text(query.output_path.to_string_lossy())),
            ("recordCount", Value::from(record_count)),
            ("sha256", text(sha256)),
        ]))
    }
}

#[async_trait::async_trait]
impl LocalAdminHandler for LocalAdminRouter {
    async fn handle(&self, request: AdminRequest) -> AdminResponse {
        let id = request.id().clone();
        let method = request.method();
        match self.dispatch(method, request.params()).await {
            Ok(result) => AdminResponse::success(id, result),
            Err(error) => {
                // 只记方法名与错误码：params 里可能有路径、命令或凭据值（§14.1）。
                tracing::warn!(
                    method = method.as_str(),
                    code = error.code().as_str(),
                    "local admin method failed"
                );
                AdminResponse::failure(id, error)
            }
        }
    }
}

/// 新的配对标识（§5.3/§5.4 的 `pairingId`）。
///
/// 由本层分配：配对行由调用方组装（`UseCases::create_pairing` 只负责提交），`IdGenerator::pairing_id`
/// 目前没有任何生产调用方；本地通道与后续的 Sync/Node Link 配对入口都从 `uuid` v4 取随机标识
/// （与 `transport::local` 的 `InstanceId`/`FacadeAttachmentId` 同一来源）。
fn new_pairing_id() -> Result<PairingId, AdminError> {
    PairingId::new(&uuid::Uuid::new_v4().to_string()).map_err(|_| {
        AdminError::new(
            LocalErrorCode::Internal,
            "pairing id generation produced a malformed uuid",
        )
    })
}

/// `T | null` 字符串（§1.1：可空字段用 `null` 而不是缺字段）。
fn optional_text(value: Option<&str>) -> Value {
    match value {
        Some(text) => Value::String(text.to_owned()),
        None => Value::Null,
    }
}

/// 撤销/确认后的读回失败：持久化状态与方法的返回值形状不一致，属内部错误。
fn not_readable(operation: &str, what: &str) -> AdminError {
    AdminError::new(
        LocalErrorCode::Internal,
        format!("{operation}: the committed {what} cannot be read back"),
    )
}

/// `*.revoke` 的目标在**调用撤销用例之前**被判为「本方法的目标域里没有它」（§7）。
///
/// 两种情形共用本错误：① 记录不存在；② 记录已是撤销终态——§7 规定 `*.revoke` 的重试得到
/// `local.not_found`，而存储层的撤销是幂等的（`COALESCE(revoked_at, ?)`），不先判就会把重试
/// 当成一次成功的新撤销返回。
fn not_revocable(operation: &str, what: &str, id: &str) -> AdminError {
    AdminError::new(
        LocalErrorCode::NotFound,
        format!("{operation}: {what} `{id}` does not exist or is already revoked"),
    )
}
