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
    Actor, AgentProfile, Fingerprint, GrantSet, NodeKind, PairingId, PairingPeer, PairingRecord,
    PairingState, PairingTarget, PeerIdentity, ProviderRef, ScopeSet, Timestamp, WorkspaceRecord,
};
use acp_core::ports::{AuditQuery, AuditStore, Clock, TrustRecordRef};
use acp_core::use_cases::UseCases;
use identity_auth::{
    IdentityKeystore, KeystoreError, PairingDecision, PairingError, PairingSpec,
    RequestedCapabilities, Sas, SecretBytes, SecretPurpose,
};
use serde_json::Value;

use crate::local_admin::audit;
use crate::local_admin::daemon::DaemonControl;
use crate::local_admin::envelope::{AdminRequest, AdminResponse, JsonObject};
use crate::local_admin::error::{AdminError, LocalErrorCode};
use crate::local_admin::handler::LocalAdminHandler;
use crate::local_admin::method::Method;
use crate::local_admin::pairing::{self, PairingSessions};
use crate::local_admin::params::{self, ProviderConfigure};
use crate::local_admin::view::{self, object, string_array, text, timestamp};

/// [`LocalAdminRouter`] 的组合根注入口。
pub struct LocalAdminDeps {
    /// Daemon 生命周期与运行期状态（由 `app::daemon` 实现）。
    pub daemon: Arc<dyn DaemonControl>,
    /// 业务用例面（本地配置族、Export/Import 族、信任族）。
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
            Method::WorkspaceSelect => self.workspace_select(params).await,
            Method::AgentConfigure => self.agent_configure(params).await,
            Method::ProviderConfigure => self.provider_configure(params).await,
            Method::ExportCreate => self.export_create(params).await,
            Method::ExportList => self.export_list(params).await,
            Method::ExportRevoke => self.export_revoke(params).await,
            Method::ImportAdd => self.import_add(params).await,
            Method::ImportList => self.import_list(params).await,
            Method::ImportRemove => self.import_remove(params).await,
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
    // 本地配置族（§5.2）
    // -----------------------------------------------------------------------------------------

    async fn workspace_select(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        let select = params::workspace_select(params)?;
        let actor = Actor::LocalCli;
        let previous = self
            .deps
            .core
            .workspace(&actor, &select.alias)
            .await
            .map_err(|error| params::map_port_error("workspace.select", error))?;
        let now = self.deps.clock.now();
        // 同一 alias 再次调用是更新：`createdAt` 保持首次写入值，`updatedAt` 用本次调用时间。
        let created_at = previous
            .as_ref()
            .map_or_else(|| now.clone(), |record| record.created_at().clone());
        let record = WorkspaceRecord::try_new(
            select.alias,
            &select.display_name,
            &select.canonical_path,
            created_at,
            now,
        )
        .map_err(|error| params::invalid_params(format!("workspace.select: {error}")))?;
        let result = object(vec![("workspace", Value::Object(view::workspace(&record)))]);
        self.deps
            .core
            .put_workspace(&actor, record)
            .await
            .map_err(|error| params::map_port_error("workspace.select", error))?;
        Ok(result)
    }

    async fn agent_configure(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        let configure = params::agent_configure(params)?;
        let actor = Actor::LocalCli;
        let previous = self
            .deps
            .core
            .profile(&actor, &configure.agent_id)
            .await
            .map_err(|error| params::map_port_error("agent.configure", error))?;
        let now = self.deps.clock.now();
        let created_at = previous
            .as_ref()
            .map_or_else(|| now.clone(), |profile| profile.created_at().clone());
        // 凭据到环境变量的绑定（`ProviderEnvBinding`）只能由种子配置导入，`agent.configure` 的
        // params 不承载它（§5.2）：更新时保留既有绑定，避免一次 profile 编辑静默清空凭据注入；
        // 若新的 `envAllowlist` 不再包含某个绑定名，`AgentProfile::try_new` 会拒绝整次写入
        // （白名单是注入上限，不允许留下越界的绑定）。
        let env = previous
            .as_ref()
            .map_or_else(Vec::new, |profile| profile.env().to_vec());
        let profile = AgentProfile::try_new(
            configure.agent_id,
            &configure.display_name,
            &configure.command,
            configure.args,
            configure.env_allowlist,
            env,
            configure.default,
            created_at,
            now,
        )
        .map_err(|error| params::invalid_params(format!("agent.configure: {error}")))?;
        let result = object(vec![("agent", Value::Object(view::agent(&profile)))]);
        self.deps
            .core
            .put_profile(&actor, profile)
            .await
            .map_err(|error| params::map_port_error("agent.configure", error))?;
        Ok(result)
    }

    async fn provider_configure(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        let configure = params::provider_configure(params)?;
        let actor = Actor::LocalCli;
        let now = self.deps.clock.now();

        let references = self
            .deps
            .core
            .provider_refs(&actor)
            .await
            .map_err(|error| params::map_port_error("provider.configure", error))?;
        let previous = references
            .iter()
            .find(|reference| {
                reference.id() == configure.provider_id && reference.kind() == configure.kind
            })
            .cloned();
        // §11.2 第 7 条：换绑必须递增 version；版本同时进入 keystore 标签，使「先写新条目、再提交
        // SQLite 引用」的提交顺序成立（提交失败时旧引用仍指向完整的旧条目）。
        let version = previous
            .as_ref()
            .map_or(1, |reference| reference.version() + 1);
        let keystore_ref = provider_keystore_ref(&configure.provider_id, version);
        let labels = self.write_credentials(&keystore_ref, &configure).await?;

        let reference = match ProviderRef::try_new(
            &configure.provider_id,
            configure.kind,
            &configure.display_name,
            configure
                .values
                .iter()
                .map(|(field, _)| field.clone())
                .collect(),
            &keystore_ref,
            version,
            now,
        ) {
            Ok(reference) => reference,
            Err(error) => {
                self.discard_credentials(&labels).await;
                return Err(params::invalid_params(format!(
                    "provider.configure: {error}"
                )));
            }
        };
        if let Err(error) = self.deps.core.put_provider_ref(&actor, reference).await {
            // 引用未提交：本次写入的条目无引用者，尽力清理（清理失败只记日志，不改变错误码）。
            self.discard_credentials(&labels).await;
            return Err(params::map_port_error("provider.configure", error));
        }

        // 新引用已提交，旧版本的条目才可清理（§11.6 第 7 条）。
        if let Some(previous) = previous {
            let stale: Vec<String> = previous
                .configured_fields()
                .iter()
                .map(|field| provider_secret_label(previous.keystore_ref(), field))
                .collect();
            self.discard_credentials(&stale).await;
        }

        Ok(object(vec![(
            "provider",
            Value::Object(view::provider(
                &configure.provider_id,
                configure.kind.as_str(),
                &configure
                    .values
                    .iter()
                    .map(|(field, _)| field.clone())
                    .collect::<Vec<_>>(),
            )),
        )]))
    }

    /// 把 `values` 里的凭据值逐项写入 keystore（先写新版本条目）；任一项失败即回滚本次已写入的条目。
    async fn write_credentials(
        &self,
        keystore_ref: &str,
        configure: &ProviderConfigure,
    ) -> Result<Vec<String>, AdminError> {
        let mut written = Vec::with_capacity(configure.values.len());
        for (field, secret) in &configure.values {
            let label = provider_secret_label(keystore_ref, field);
            let value = SecretBytes::new(secret.as_bytes());
            if let Err(error) = self
                .deps
                .keystore
                .put_secret(SecretPurpose::ProviderCredential, &label, &value)
                .await
            {
                self.discard_credentials(&written).await;
                return Err(map_keystore_error(error));
            }
            written.push(label);
        }
        Ok(written)
    }

    /// 尽力删除给定条目（回滚或清理旧版本用）。失败只记结构化日志：新引用已经有效，
    /// 残留条目是无引用者，不得把「清理失败」报成方法失败（§11.6 第 7 条）。
    async fn discard_credentials(&self, labels: &[String]) {
        for label in labels {
            if let Err(error) = self
                .deps
                .keystore
                .delete_secret(SecretPurpose::ProviderCredential, label)
                .await
            {
                tracing::warn!(
                    error = %error,
                    "local admin keystore entry cleanup failed"
                );
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Export/Import（§5.5）
    // -----------------------------------------------------------------------------------------

    async fn export_create(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        let actor = Actor::LocalCli;
        let record = params::export_create(params, self.deps.clock.now())?;
        if self
            .deps
            .core
            .export(&actor, record.export_id())
            .await
            .map_err(|error| params::map_port_error("export.create", error))?
            .is_some()
        {
            return Err(AdminError::new(
                LocalErrorCode::Conflict,
                "export.create: exportId already exists",
            ));
        }
        // §5.5：每个 alias 必须已在 `owned_workspace` 建立（Export 只引用符号名，不复制路径）。
        for entry in record.workspace_aliases() {
            let alias = entry.alias().clone();
            if self
                .deps
                .core
                .workspace(&actor, &alias)
                .await
                .map_err(|error| params::map_port_error("export.create", error))?
                .is_none()
            {
                return Err(AdminError::new(
                    LocalErrorCode::NotFound,
                    format!(
                        "export.create: workspace alias `{alias}` is not registered on this node"
                    ),
                ));
            }
        }
        let result = object(vec![("export", Value::Object(view::export(&record)))]);
        self.deps
            .core
            .put_export(&actor, record)
            .await
            .map_err(|error| params::map_port_error("export.create", error))?;
        Ok(result)
    }

    async fn export_list(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        params::reject_unknown_fields(params, &[])?;
        let actor = Actor::LocalCli;
        let exports = self
            .deps
            .core
            .exports(&actor)
            .await
            .map_err(|error| params::map_port_error("export.list", error))?;
        Ok(object(vec![(
            "exports",
            Value::Array(
                exports
                    .iter()
                    .map(|record| Value::Object(view::export(record)))
                    .collect(),
            ),
        )]))
    }

    /// `export.revoke`（§5.5）：先判定「存在且未撤销」，再提交撤销并读回持久时间。
    ///
    /// 读-写之间的竞态无害：并发重复撤销最坏落到存储的幂等撤销，结果仍是同一条已撤销记录。
    async fn export_revoke(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "export.revoke";
        let actor = Actor::LocalCli;
        let export_id = params::export_revoke(params)?;
        let revocable = self
            .deps
            .core
            .export(&actor, &export_id)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?
            .is_some_and(|record| !record.is_revoked());
        if !revocable {
            return Err(not_revocable(OPERATION, "export", export_id.as_str()));
        }
        self.deps
            .core
            .revoke_export(&actor, &export_id)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?;
        // §5.5：必须在持久状态提交后才返回。撤销时间取读回的持久值，不由本层时钟猜。
        let record = self
            .deps
            .core
            .export(&actor, &export_id)
            .await
            .map_err(|error| params::map_port_error(OPERATION, error))?;
        let revoked_at = record
            .as_ref()
            .and_then(|record| record.revoked_at())
            .ok_or_else(|| {
                AdminError::new(
                    LocalErrorCode::Internal,
                    "export.revoke: revoked export cannot be read back",
                )
            })?;
        Ok(object(vec![
            ("exportId", text(export_id.as_str())),
            ("revokedAt", timestamp(revoked_at)),
        ]))
    }

    /// §5.5：本切片没有可用的 Node Link catalog 快照，因此**恒**返回 `local.unavailable`
    /// （可重试，不是「不存在」）：无论参数是否合法都不校验、不写记录。
    async fn import_add(&self, _params: &JsonObject) -> Result<JsonObject, AdminError> {
        Err(AdminError::new(
            LocalErrorCode::Unavailable,
            "import.add: no node link catalog snapshot is available on this node yet",
        ))
    }

    async fn import_list(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        params::reject_unknown_fields(params, &[])?;
        let actor = Actor::LocalCli;
        let imports = self
            .deps
            .core
            .imports(&actor)
            .await
            .map_err(|error| params::map_port_error("import.list", error))?;
        Ok(object(vec![(
            "imports",
            Value::Array(
                imports
                    .iter()
                    .map(|record| Value::Object(view::import(record)))
                    .collect(),
            ),
        )]))
    }

    async fn import_remove(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        let actor = Actor::LocalCli;
        let import_id = params::import_remove(params)?;
        self.deps
            .core
            .remove_import(&actor, &import_id)
            .await
            .map_err(|error| params::map_port_error("import.remove", error))?;
        Ok(object(vec![]))
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
                &RequestedCapabilities {
                    scopes: begin.scopes.clone(),
                    grants: GrantSet::empty(),
                },
                // §5.3 的 `device.pair.begin` 不携带展示名：对端名称由 claim 声明。
                None,
                &now,
                &expires_at,
            )
            .map_err(|error| params::map_pairing_error(OPERATION, error))?;
        let pairing_url = self.deps.pairing.device_pairing_url(
            OPERATION,
            &pairing_id,
            &draft.secret,
            &expires_at,
            &host_public_key,
        )?;
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
            ("expiresAt", timestamp(&expires_at)),
            ("scopes", string_array(begin.scopes.iter())),
        ]))
    }

    /// `device.pair.status`（§5.3）：claim 前只有 `state`/`expiresAt`，claim 后才是名称/指纹/SAS/请求 scopes。
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
            ("sas", optional_text(view.sas.as_ref().map(Sas::as_str))),
            (
                "requestedScopes",
                text_array_or_null(
                    view.requested
                        .as_ref()
                        .map(|requested| requested.scopes.iter()),
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

    /// `device.pair.confirm`（§5.3）：以用户确认的最终 scopes 创建 active 设备记录。
    async fn device_pair_confirm(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "device.pair.confirm";
        let confirm = params::device_pair_confirm(params)?;
        let record = self
            .require_pairing(OPERATION, &confirm.pairing_id, PairingTarget::Device)
            .await?;
        let now = self.deps.clock.now();
        params::require_pending_confirmation(OPERATION, &record, &now)?;
        // 确认集合必须完整展示后提交（§5.3），且不得超出请求值（超出是参数越界）。
        params::require_confirm_subset(
            OPERATION,
            "scopes",
            confirm.scopes.iter(),
            record.requested_scopes().iter(),
        )?;
        let decision = PairingDecision::Approve {
            granted_scopes: confirm.scopes.clone(),
            granted_grants: GrantSet::empty(),
        };
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
            ("scopes", string_array(confirm.scopes.iter())),
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
        let params::NodePairBegin::Owner {
            display_name,
            grants,
        } = params::node_pair_begin(params)?
        else {
            // §5.4：`mode = "access"` 需要对 Owner 执行 HTTPS claim，本切片未落地。这里不构造任何
            // HTTP 调用、不写任何状态，也不另行填充参数形状（与 §5.7 的 `node.rotate-key.begin` 同款）。
            return Err(AdminError::new(
                LocalErrorCode::Unsupported,
                format!(
                    "{OPERATION}: mode `access` is not implemented yet (no outbound claim is issued)"
                ),
            ));
        };
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
                &RequestedCapabilities {
                    scopes: ScopeSet::empty(),
                    grants,
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
            // claim 之前不知道对端身份，也没有 SAS（双方各自计算）。
            ("sas", Value::Null),
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
            ("sas", optional_text(view.sas.as_ref().map(Sas::as_str))),
            (
                "requestedGrants",
                text_array_or_null(
                    view.requested
                        .as_ref()
                        .map(|requested| requested.grants.iter()),
                ),
            ),
            ("expiresAt", timestamp(&view.expires_at)),
            ("lastRefreshError", Value::Null),
        ]))
    }

    /// `node.pair.confirm`（§5.4）：分配初始 `grant.*` 并创建信任记录。
    async fn node_pair_confirm(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        const OPERATION: &str = "node.pair.confirm";
        let confirm = params::node_pair_confirm(params)?;
        let record = self
            .require_pairing(OPERATION, &confirm.pairing_id, PairingTarget::Node)
            .await?;
        let now = self.deps.clock.now();
        params::require_pending_confirmation(OPERATION, &record, &now)?;
        params::require_confirm_subset(
            OPERATION,
            "grants",
            confirm.grants.iter(),
            record.requested_grants().iter(),
        )?;
        let decision = PairingDecision::Approve {
            granted_scopes: ScopeSet::empty(),
            granted_grants: confirm.grants.clone(),
        };
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
        Ok(object(vec![
            ("nodeId", text(node_id.as_str())),
            ("grants", string_array(confirm.grants.iter())),
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

    /// 状态视图：记录 + 已认领的对端事实 + 主机侧 SAS，交给状态机投影成 §5.3/§5.4 的可见字段。
    async fn pairing_view(
        &self,
        operation: &str,
        pairing_id: &PairingId,
        target: PairingTarget,
    ) -> Result<identity_auth::PairingStatusView, AdminError> {
        let record = self.require_pairing(operation, pairing_id, target).await?;
        let peer = if record.claimed_at().is_some() {
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
        let sas = self.host_sas(operation, &record, peer.as_ref()).await?;
        let claimed = peer
            .as_ref()
            .map(|peer| pairing::claimed_pairing(&record, peer));
        Ok(self
            .deps
            .pairing
            .authority()
            .pairing_status(&record, claimed.as_ref(), sas))
    }

    /// 主机侧 SAS：只在 `pending_confirmation` 且内存仍持有 secret 时派生。
    ///
    /// secret 已清除（重启后被清理、或已过期）时按 `null` 呈现并记一条结构化日志：状态本身仍然可查
    /// （CLI 需要看到终态），但**绝不**伪造 SAS；其它失败是实现缺陷，直接回 `local.internal`。
    async fn host_sas(
        &self,
        operation: &str,
        record: &PairingRecord,
        peer: Option<&PairingPeer>,
    ) -> Result<Option<Sas>, AdminError> {
        if record.state() != PairingState::PendingConfirmation {
            return Ok(None);
        }
        let Some(peer) = peer else {
            return Ok(None);
        };
        match self.deps.pairing.host_sas(record, peer).await {
            Ok(sas) => Ok(Some(sas)),
            Err(PairingError::SecretUnavailable) => {
                tracing::warn!(
                    method = operation,
                    "pairing secret is no longer held in memory"
                );
                Ok(None)
            }
            Err(error) => Err(params::map_pairing_error(operation, error)),
        }
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

/// `string[] | null`。
fn text_array_or_null<'a>(items: Option<impl IntoIterator<Item = &'a str>>) -> Value {
    match items {
        Some(items) => Value::Array(items.into_iter().map(text).collect()),
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

/// Provider 凭据的 keystore 条目引用（§5.2 的 `keystore_ref`）：
/// `<providerDigest>@v<version>`，其中 `providerDigest` 是 `providerId` 的 SHA-256 前 16 个 hex 字符。
///
/// 为什么用摘要而不是 `providerId` 本身：keystore 的标签上限是 128 字节，而 `providerId` 与字段名
/// 各自可达 64 字符，`<providerId>.<field>` 的拼接会越界；摘要把标识收敛为定宽。
/// 每个字段的条目标签是 [`provider_secret_label`]，组合根用 `ProviderRef.keystore_ref` 加字段名即可定位。
fn provider_keystore_ref(provider_id: &str, version: u64) -> String {
    format!("{}@v{version}", sha256_hex_prefix(provider_id))
}

/// 某个字段的 keystore 标签：`<keystore_ref>.<fieldName>`。
fn provider_secret_label(keystore_ref: &str, field: &str) -> String {
    format!("{keystore_ref}.{field}")
}

/// `text` 的 SHA-256 前 16 个 hex 字符（小写）。
fn sha256_hex_prefix(text: &str) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(text.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex.truncate(16);
    hex
}

/// keystore 失败 → `local.*`：不可用即 `local.unavailable`（正式模式不得降级为明文存储，§5.2）；
/// 其余（条目非法、用途不符等）是实现缺陷，收敛成 `local.internal`，错误消息不转述内层文本。
fn map_keystore_error(error: KeystoreError) -> AdminError {
    match error {
        KeystoreError::Unavailable => AdminError::new(
            LocalErrorCode::Unavailable,
            "provider.configure: platform keystore is not available",
        ),
        _ => AdminError::new(
            LocalErrorCode::Internal,
            "provider.configure: platform keystore rejected the credential entry",
        ),
    }
}

#[cfg(test)]
mod tests {
    use acp_core::model::PortError;
    use base64::Engine as _;
    use serde_json::json;

    use super::*;
    use crate::local_admin::envelope::{AdminOutcome, RequestId, decode_request};
    use crate::local_admin::method::Method;
    use crate::local_admin::test_support::TestWorld;
    use acp_core::model::{AgentId, ProviderEnvBinding};

    fn request(method: Method, params: serde_json::Value) -> AdminRequest {
        let bytes = serde_json::to_vec(&json!({
            "v": 1,
            "id": "2ae1c07c-0000-4000-8000-0000000000ff",
            "method": method.as_str(),
            "params": params,
        }))
        .expect("请求信封可序列化");
        decode_request(&bytes).expect("信封合法")
    }

    fn result_of(response: &AdminResponse) -> JsonObject {
        match response.outcome() {
            AdminOutcome::Success { result } => result.clone(),
            AdminOutcome::Failure { error } => {
                panic!(
                    "期望成功响应，实际 {}：{}",
                    error.code().as_str(),
                    error.message()
                )
            }
        }
    }

    fn error_of(response: &AdminResponse) -> (LocalErrorCode, String) {
        match response.outcome() {
            AdminOutcome::Failure { error } => (error.code(), error.message().to_owned()),
            AdminOutcome::Success { result } => panic!("期望失败响应，实际 {result:?}"),
        }
    }

    fn export_params(world: &TestWorld) -> serde_json::Value {
        world.write_workspace("project", "Project");
        json!({
            "exportId": "export-1",
            "displayName": "Export One",
            "agentIds": ["codex"],
            "workspaceAliases": [{ "alias": "project", "displayName": "Project" }],
            "defaultWorkspaceAlias": "project",
            "templates": [{
                "templateId": "template-1",
                "displayName": "Template One",
                "workspaceAlias": "project",
                "params": [],
            }],
            "defaultTemplateId": "template-1",
            "scopes": ["grant.session.read"],
            "cachePolicy": "no-content-cache",
        })
    }

    #[tokio::test]
    async fn daemon_status_returns_the_daemon_control_snapshot() {
        let world = TestWorld::new();
        world.daemon.set_version("9.9.9");
        let router = world.router();
        let response = router
            .handle(request(Method::DaemonStatus, json!({})))
            .await;
        assert_eq!(
            response.id().as_str(),
            "2ae1c07c-0000-4000-8000-0000000000ff"
        );
        let result = result_of(&response);
        assert_eq!(result["version"], json!("9.9.9"));
        assert_eq!(result["instanceId"], json!("0123456789abcdef"));
        assert_eq!(result["agents"][0]["agentId"], json!("codex"));
        assert_eq!(result["counts"]["exports"], json!(0));
        assert_eq!(result["listen"], json!([]));
    }

    #[tokio::test]
    async fn daemon_status_rejects_params_and_daemon_stop_validates_the_grace() {
        let world = TestWorld::new();
        let router = world.router();
        let (code, _) = error_of(
            &router
                .handle(request(Method::DaemonStatus, json!({"a": 1})))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);

        for rejected in [
            json!({"graceMs": 60_001}),
            json!({"graceMs": -1}),
            json!({}),
        ] {
            let (code, _) = error_of(
                &router
                    .handle(request(Method::DaemonStop, rejected.clone()))
                    .await,
            );
            assert_eq!(code, LocalErrorCode::InvalidParams, "{rejected}");
        }
        assert!(world.daemon.stops().is_empty(), "非法参数不得触发停止");

        let result = result_of(
            &router
                .handle(request(Method::DaemonStop, json!({"graceMs": 1500})))
                .await,
        );
        assert_eq!(result["accepted"], json!(true));
        assert_eq!(world.daemon.stops(), vec![Some(1500)]);

        let result = result_of(
            &router
                .handle(request(Method::DaemonStop, json!({"graceMs": null})))
                .await,
        );
        assert_eq!(result["accepted"], json!(true));
        assert_eq!(world.daemon.stops(), vec![Some(1500), None]);
    }

    #[tokio::test]
    async fn daemon_stop_propagates_a_composition_root_failure() {
        let world = TestWorld::new();
        world.daemon.fail_next_stop();
        let router = world.router();
        let (code, message) = error_of(
            &router
                .handle(request(Method::DaemonStop, json!({"graceMs": 0})))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Unavailable);
        assert!(message.contains("shutting down"), "{message}");
    }

    #[tokio::test]
    async fn workspace_select_persists_the_canonical_path_and_keeps_created_at() {
        let world = TestWorld::new();
        let router = world.router();
        let directory = world.temporary_directory();
        let root = directory.to_str().expect("utf-8").to_owned();

        let first = result_of(
            &router
                .handle(request(
                    Method::WorkspaceSelect,
                    json!({ "alias": "project", "displayName": "P", "rootPath": root }),
                ))
                .await,
        );
        let canonical = std::fs::canonicalize(&directory).expect("可规范化");
        assert_eq!(first["workspace"]["alias"], json!("project"));
        assert_eq!(
            first["workspace"]["rootPath"],
            json!(canonical.to_str().expect("utf-8"))
        );
        assert_eq!(first["workspace"]["createdAt"], json!(world.clock_text()));
        assert_eq!(
            world.config.workspace_path("project").as_deref(),
            canonical.to_str()
        );

        world.clock.set("2026-09-18T10:00:00.000Z");
        let second = result_of(
            &router
                .handle(request(
                    Method::WorkspaceSelect,
                    json!({ "alias": "project", "displayName": "P2", "rootPath": root }),
                ))
                .await,
        );
        assert_eq!(
            second["workspace"]["createdAt"],
            json!("2026-09-18T09:12:03.412Z"),
            "同一 alias 是更新，createdAt 保持首次写入值"
        );
        assert_eq!(world.config.profile_count(), 0);

        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::WorkspaceSelect,
                    json!({ "alias": "project", "displayName": "P", "rootPath": "relative" }),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
        std::fs::remove_dir_all(&directory).ok();
    }

    #[tokio::test]
    async fn workspace_select_maps_storage_failures() {
        let world = TestWorld::new();
        world
            .config
            .fail_next_workspace_lookup(PortError::Unavailable(
                acp_core::model::UnavailableKind::IoError,
            ));
        let router = world.router();
        let directory = world.temporary_directory();
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::WorkspaceSelect,
                    json!({
                        "alias": "project",
                        "displayName": "P",
                        "rootPath": directory.to_str().expect("utf-8"),
                    }),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Unavailable);
        std::fs::remove_dir_all(&directory).ok();
    }

    #[tokio::test]
    async fn agent_configure_writes_the_profile_and_reports_the_result_shape() {
        let world = TestWorld::new();
        let router = world.router();
        let params = json!({
            "agentId": "codex",
            "displayName": "Codex",
            "command": "codex",
            "args": ["--acp"],
            "envAllowlist": ["OPENAI_API_KEY"],
            "default": true,
        });
        let result = result_of(
            &router
                .handle(request(Method::AgentConfigure, params.clone()))
                .await,
        );
        assert_eq!(
            result["agent"],
            json!({
                "agentId": "codex",
                "displayName": "Codex",
                "default": true,
            })
        );
        assert_eq!(world.config.profile_count(), 1);
        assert_eq!(world.config.profile_default("codex"), Some(true));

        // 语法合法的 agentId 但缺少必需字段 → 参数错误，不产生写入。
        let mut missing = params;
        missing.as_object_mut().expect("object").remove("command");
        let (code, _) = error_of(
            &router
                .handle(request(Method::AgentConfigure, missing))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
        assert_eq!(world.config.profile_count(), 1, "校验失败不得写入");
    }

    #[tokio::test]
    async fn provider_configure_writes_credentials_to_the_keystore_only() {
        let world = TestWorld::new();
        let router = world.router();
        let params = json!({
            "providerId": "openai.primary",
            "kind": "provider",
            "displayName": "OpenAI",
            "values": { "api_key": "s3cret-value", "org": "acme" },
        });
        let response = router
            .handle(request(Method::ProviderConfigure, params.clone()))
            .await;
        let result = result_of(&response);
        assert_eq!(result["provider"]["providerId"], json!("openai.primary"));
        assert_eq!(result["provider"]["kind"], json!("provider"));
        assert_eq!(
            result["provider"]["configuredFields"],
            json!(["api_key", "org"])
        );

        // result 与整条响应里都不得出现凭据值（§4 规则 6、§5.2）。
        let encoded = String::from_utf8(response.encode().expect("可编码")).expect("utf-8");
        assert!(!encoded.contains("s3cret-value"), "{encoded}");
        assert!(!encoded.contains("acme"), "{encoded}");

        // 凭据只进 keystore；SQLite 侧只有字段名、引用与版本。
        let label = format!("{}@v1.api_key", sha256_hex_prefix("openai.primary"));
        assert_eq!(
            world.keystore.secret(&label).as_deref(),
            Some("s3cret-value")
        );
        assert_eq!(
            world
                .keystore
                .secret(&format!("{}@v1.org", sha256_hex_prefix("openai.primary")))
                .as_deref(),
            Some("acme")
        );
        let recorded = world.config.provider_ref("openai.primary");
        let (fields, keystore_ref, version) = recorded.expect("引用已提交");
        assert_eq!(fields, vec!["api_key".to_owned(), "org".to_owned()]);
        assert_eq!(
            keystore_ref,
            format!("{}@v1", sha256_hex_prefix("openai.primary"))
        );
        assert_eq!(version, 1);
        assert!(!keystore_ref.contains("s3cret-value"));

        // 再次配置：version 递增、新标签写入、旧标签清理。
        let result = result_of(
            &router
                .handle(request(
                    Method::ProviderConfigure,
                    json!({
                        "providerId": "openai.primary",
                        "kind": "provider",
                        "displayName": "OpenAI",
                        "values": { "api_key": "rotated" },
                    }),
                ))
                .await,
        );
        assert_eq!(result["provider"]["configuredFields"], json!(["api_key"]));
        assert_eq!(
            world
                .keystore
                .secret(&format!(
                    "{}@v2.api_key",
                    sha256_hex_prefix("openai.primary")
                ))
                .as_deref(),
            Some("rotated")
        );
        assert!(
            world.keystore.secret(&label).is_none(),
            "新引用提交后旧版本条目被清理"
        );
        assert_eq!(
            world.config.provider_ref("openai.primary").expect("引用").2,
            2
        );
    }

    #[tokio::test]
    async fn provider_configure_rejects_bad_params_and_unavailable_keystore() {
        let world = TestWorld::new();
        let router = world.router();
        for rejected in [
            json!({ "providerId": "p", "kind": "provider", "displayName": "P", "values": {} }),
            json!({ "providerId": "p", "kind": "other", "displayName": "P", "values": {"k": "v"} }),
        ] {
            let (code, _) = error_of(
                &router
                    .handle(request(Method::ProviderConfigure, rejected.clone()))
                    .await,
            );
            assert_eq!(code, LocalErrorCode::InvalidParams, "{rejected}");
        }
        assert!(world.keystore.is_empty(), "参数非法不得写 keystore");

        world.keystore.set_available(false);
        let (code, message) = error_of(
            &router
                .handle(request(
                    Method::ProviderConfigure,
                    json!({
                        "providerId": "p",
                        "kind": "provider",
                        "displayName": "P",
                        "values": {"k": "v"},
                    }),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Unavailable);
        assert!(message.contains("keystore"), "{message}");
        assert!(world.config.provider_ref("p").is_none(), "不得落库");
        assert!(world.keystore.is_empty(), "不得降级为明文存储");
    }

    #[tokio::test]
    async fn agent_configure_preserves_existing_provider_env_bindings() {
        let world = TestWorld::new();
        let router = world.router();
        // 种子 profile（只能由配置文件导入）带凭据到环境变量的绑定。
        world.config.seed_profile(
            AgentProfile::try_new(
                AgentId::new("codex").expect("agent id"),
                "Codex",
                "codex",
                vec![],
                vec!["OPENAI_API_KEY".to_owned()],
                vec![
                    ProviderEnvBinding::try_new("openai.primary", "api_key", "OPENAI_API_KEY")
                        .expect("binding"),
                ],
                true,
                world.clock.now(),
                world.clock.now(),
            )
            .expect("种子 profile"),
        );

        let result = result_of(
            &router
                .handle(request(
                    Method::AgentConfigure,
                    json!({
                        "agentId": "codex",
                        "displayName": "Codex Two",
                        "command": "codex",
                        "args": ["--acp"],
                        "envAllowlist": ["OPENAI_API_KEY"],
                        "default": true,
                    }),
                ))
                .await,
        );
        assert_eq!(result["agent"]["displayName"], json!("Codex Two"));
        let stored = world.config.profile("codex").expect("profile");
        assert_eq!(
            stored.env().len(),
            1,
            "更新 profile 不得静默清空凭据注入绑定"
        );

        // 缩短白名单会与既有绑定冲突：整次写入被拒（白名单是注入上限）。
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::AgentConfigure,
                    json!({
                        "agentId": "codex",
                        "displayName": "Codex Three",
                        "command": "codex",
                        "args": [],
                        "envAllowlist": [],
                        "default": true,
                    }),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
        assert_eq!(
            world
                .config
                .profile("codex")
                .expect("profile")
                .display_name(),
            "Codex Two",
            "被拒的写入不得部分生效"
        );
    }

    #[tokio::test]
    async fn provider_configure_rolls_back_credentials_when_a_later_step_fails() {
        // ① 第二个字段写 keystore 失败：第一个字段的条目必须被回滚，且不写引用。
        let world = TestWorld::new();
        let router = world.router();
        let rollback_label = format!("{}@v1.org", sha256_hex_prefix("openai.primary"));
        world.keystore.fail_put_for(&rollback_label);
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::ProviderConfigure,
                    json!({
                        "providerId": "openai.primary",
                        "kind": "provider",
                        "displayName": "OpenAI",
                        "values": { "api_key": "s3cret", "org": "acme" },
                    }),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Unavailable);
        assert!(world.keystore.is_empty(), "已写入的条目必须回滚");
        assert!(world.config.provider_ref("openai.primary").is_none());

        // ② keystore 写入成功但引用提交失败：同样回滚本次条目，错误码来自 PortError 映射。
        let world = TestWorld::new();
        world
            .config
            .fail_next_provider_ref(PortError::Backend(Box::new(std::io::Error::other(
                "INSERT INTO owned_provider_ref failed at D:\\secret\\db.sqlite",
            ))));
        let router = world.router();
        let (code, message) = error_of(
            &router
                .handle(request(
                    Method::ProviderConfigure,
                    json!({
                        "providerId": "openai.primary",
                        "kind": "provider",
                        "displayName": "OpenAI",
                        "values": { "api_key": "s3cret" },
                    }),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Internal);
        assert!(!message.contains("db.sqlite"), "{message}");
        assert!(world.keystore.is_empty(), "引用未提交的条目必须清理");
        assert!(world.config.provider_ref("openai.primary").is_none());
    }

    #[tokio::test]
    async fn export_create_requires_registered_aliases_and_a_free_id() {
        let world = TestWorld::new();
        let router = world.router();

        // 未建立 alias → local.not_found，不写记录。
        let unknown_alias = json!({
            "exportId": "export-1",
            "displayName": "Export One",
            "agentIds": ["codex"],
            "workspaceAliases": [{ "alias": "project", "displayName": "Project" }],
            "defaultWorkspaceAlias": "project",
            "templates": [{
                "templateId": "template-1",
                "displayName": "Template One",
                "workspaceAlias": "project",
                "params": [],
            }],
            "defaultTemplateId": "template-1",
            "scopes": [],
            "cachePolicy": "no-content-cache",
        });
        let (code, message) = error_of(
            &router
                .handle(request(Method::ExportCreate, unknown_alias.clone()))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
        assert!(message.contains("project"), "{message}");
        assert_eq!(world.exports.count(), 0);

        let params = export_params(&world);
        let result = result_of(
            &router
                .handle(request(Method::ExportCreate, params.clone()))
                .await,
        );
        assert_eq!(result["export"]["exportId"], json!("export-1"));
        assert_eq!(result["export"]["revokedAt"], json!(null));
        assert_eq!(result["export"]["createdAt"], json!(world.clock_text()));
        assert_eq!(result["export"]["agentIds"], json!(["codex"]));
        assert_eq!(result["export"]["scopes"], json!(["grant.session.read"]));
        assert_eq!(result["export"]["cachePolicy"], json!("no-content-cache"));
        assert_eq!(world.exports.count(), 1);

        // 同一个 exportId 再次创建 → local.conflict（§5.5 的重试语义）。
        let (code, _) = error_of(&router.handle(request(Method::ExportCreate, params)).await);
        assert_eq!(code, LocalErrorCode::Conflict);
        assert_eq!(world.exports.count(), 1);
    }

    #[tokio::test]
    async fn export_list_includes_revoked_records_and_export_revoke_returns_the_persisted_time() {
        let world = TestWorld::new();
        world.exports.seed_revoked("export-old");
        let router = world.router();
        world.write_workspace("project", "Project");
        let created = result_of(
            &router
                .handle(request(Method::ExportCreate, export_params(&world)))
                .await,
        );
        assert_eq!(created["export"]["revokedAt"], json!(null));

        let listed = result_of(&router.handle(request(Method::ExportList, json!({}))).await);
        let exports = listed["exports"].as_array().expect("数组");
        assert_eq!(exports.len(), 2, "已撤销项也在列表里");
        assert_eq!(exports[0]["exportId"], json!("export-1"));
        assert_eq!(exports[1]["revokedAt"], json!("2026-01-01T00:00:00.000Z"));

        // 撤销：返回持久化后的撤销时间。
        let revoked = result_of(
            &router
                .handle(request(
                    Method::ExportRevoke,
                    json!({"exportId": "export-1"}),
                ))
                .await,
        );
        assert_eq!(revoked["exportId"], json!("export-1"));
        assert_eq!(revoked["revokedAt"], json!(world.clock_text()));

        // 重试同一个撤销 → local.not_found（§7）：存储层撤销是幂等的，判定在调用用例之前。
        let (code, message) = error_of(
            &router
                .handle(request(
                    Method::ExportRevoke,
                    json!({"exportId": "export-1"}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
        assert!(message.contains("export-1"), "{message}");

        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::ExportRevoke,
                    json!({"exportId": "unknown"}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);

        let (code, _) = error_of(
            &router
                .handle(request(Method::ExportList, json!({"a": 1})))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
    }

    #[tokio::test]
    async fn import_add_is_always_unavailable_and_import_remove_maps_not_found() {
        let world = TestWorld::new();
        let router = world.router();

        // 参数无论是否合法都返回 local.unavailable，且不写记录（§5.5、spec 的场景）。
        for params in [
            json!({}),
            json!({
                "importId": "import-1",
                "ownerEndpoint": "wss://owner.example/",
                "ownerNodeId": "2ae1c07c-0000-4000-8000-000000000002",
                "exportIds": ["export-1"],
                "grants": ["grant.session.read"],
            }),
        ] {
            let (code, message) = error_of(
                &router
                    .handle(request(Method::ImportAdd, params.clone()))
                    .await,
            );
            assert_eq!(code, LocalErrorCode::Unavailable, "{params}");
            assert!(message.contains("catalog snapshot"), "{message}");
        }
        assert_eq!(world.exports.import_count(), 0);

        world.exports.seed_import("import-1");
        let listed = result_of(&router.handle(request(Method::ImportList, json!({}))).await);
        assert_eq!(listed["imports"][0]["importId"], json!("import-1"));
        assert_eq!(
            listed["imports"][0]["ownerEndpoint"],
            json!("wss://owner.example/")
        );
        assert_eq!(
            listed["imports"][0]["ownerNodeId"],
            json!("2ae1c07c-0000-4000-8000-000000000002")
        );
        assert_eq!(
            listed["imports"][0]["grants"],
            json!(["grant.session.read"])
        );

        let removed = result_of(
            &router
                .handle(request(
                    Method::ImportRemove,
                    json!({"importId": "import-1"}),
                ))
                .await,
        );
        assert!(removed.is_empty(), "import.remove 的 result 是空对象");
        assert_eq!(world.exports.import_count(), 0);

        // 重试同一个 remove → local.not_found（§7）。
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::ImportRemove,
                    json!({"importId": "import-1"}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
        let (code, _) = error_of(
            &router
                .handle(request(Method::ImportRemove, json!({"importId": "bad id"})))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
    }

    #[tokio::test]
    async fn import_list_maps_a_dependency_failure_without_leaking_details() {
        let world = TestWorld::new();
        world
            .exports
            .fail_next_imports(PortError::Backend(Box::new(std::io::Error::other(
                "SELECT * FROM imported_import; D:\\secret\\db.sqlite",
            ))));
        let router = world.router();
        let (code, message) =
            error_of(&router.handle(request(Method::ImportList, json!({}))).await);
        assert_eq!(code, LocalErrorCode::Internal);
        assert!(!message.contains("SELECT"), "{message}");
        assert!(!message.contains("db.sqlite"), "{message}");
    }

    #[tokio::test]
    async fn audit_export_writes_only_metadata_and_reports_the_digest() {
        let world = TestWorld::new();
        world.audit.seed_default();
        let router = world.router();
        let path = world.temporary_directory().join("audit.jsonl");

        let result = result_of(
            &router
                .handle(request(
                    Method::AuditExport,
                    json!({
                        "outputPath": path.to_str().expect("utf-8"),
                        "format": "jsonl",
                        "since": "2026-09-18T00:00:00.000Z",
                        "until": null,
                        "categories": ["device.revoked"],
                    }),
                ))
                .await,
        );
        assert_eq!(result["recordCount"], json!(1));
        assert_eq!(result["outputPath"], json!(path.to_str().expect("utf-8")));
        let content = std::fs::read_to_string(&path).expect("可读回");
        let line: Value =
            serde_json::from_str(content.lines().next().expect("一行")).expect("JSON");
        assert_eq!(line["action"], json!("device.revoked"));
        assert_eq!(line["at"], json!("2026-09-18T09:12:03.412Z"));
        assert_eq!(line["actorId"], json!("cli"));
        assert_eq!(line.as_object().expect("object").len(), 10);

        // sha256 与文件内容一致。
        use sha2::Digest as _;
        let digest = sha2::Sha256::digest(content.as_bytes());
        let expected: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(result["sha256"], json!(expected));

        // 输出文件已存在 → local.conflict（§5.6），已有文件不被覆盖。
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::AuditExport,
                    json!({
                        "outputPath": path.to_str().expect("utf-8"),
                        "format": "jsonl",
                        "since": null,
                        "until": null,
                        "categories": [],
                    }),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Conflict);
        assert_eq!(std::fs::read_to_string(&path).expect("可读回"), content);

        // 空类别数组显式给出 = 不过滤类别。
        let all = world.temporary_directory().join("audit.csv");
        let result = result_of(
            &router
                .handle(request(
                    Method::AuditExport,
                    json!({
                        "outputPath": all.to_str().expect("utf-8"),
                        "format": "csv",
                        "since": null,
                        "until": null,
                        "categories": [],
                    }),
                ))
                .await,
        );
        assert_eq!(result["recordCount"], json!(2));
        let csv = std::fs::read_to_string(&all).expect("可读回");
        assert!(csv.starts_with("\"at\",\"action\","));
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&all).ok();
    }

    #[tokio::test]
    async fn audit_export_maps_a_store_failure() {
        let world = TestWorld::new();
        world.audit.fail_next_query(PortError::Unavailable(
            acp_core::model::UnavailableKind::IoError,
        ));
        let router = world.router();
        let path = world.temporary_directory().join("audit.jsonl");
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::AuditExport,
                    json!({
                        "outputPath": path.to_str().expect("utf-8"),
                        "format": "jsonl",
                        "since": null,
                        "until": null,
                        "categories": [],
                    }),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Unavailable);
        assert!(!path.exists(), "查询失败不得留下输出文件");
    }

    #[tokio::test]
    async fn unimplemented_methods_answer_local_unsupported() {
        let world = TestWorld::new();
        let router = world.router();
        // §5.7：`node.rotate-key.begin` 是方法集里唯一尚未实现的方法（字段定义与
        // `NODE_LINK_PROTOCOL.md` §12.3 同批落地前，调用恒回 `local.unsupported`）。
        let (code, message) = error_of(
            &router
                .handle(request(Method::NodeRotateKeyBegin, json!({})))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Unsupported);
        assert!(message.contains("node.rotate-key.begin"), "{message}");

        // 设备与节点配对族（2.11）已实现：它们不再回 `local.unsupported`，而是按参数/目标回答。
        for method in [
            Method::DevicePairBegin,
            Method::DevicePairStatus,
            Method::DevicePairConfirm,
            Method::DevicePairReject,
            Method::NodePairStatus,
            Method::NodePairConfirm,
            Method::NodePairReject,
        ] {
            let (code, _) = error_of(&router.handle(request(method, json!({}))).await);
            assert_eq!(code, LocalErrorCode::InvalidParams, "{method:?}");
        }
        // `mode` 缺失时不能假定为 `access`（那是「不支持」），缺字段仍是参数错误。
        let (code, _) = error_of(
            &router
                .handle(request(Method::NodePairBegin, json!({})))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
    }

    #[tokio::test]
    async fn responses_always_carry_the_request_id_and_a_shape_valid_envelope() {
        let world = TestWorld::new();
        let router = world.router();
        for (method, params) in [
            (Method::DaemonStatus, json!({})),
            (Method::ImportAdd, json!({})),
            (
                Method::WorkspaceSelect,
                json!({"alias": "a", "displayName": "A"}),
            ),
        ] {
            let response = router.handle(request(method, params)).await;
            assert_eq!(
                response.id().as_str(),
                "2ae1c07c-0000-4000-8000-0000000000ff",
                "{method:?}"
            );
            let encoded = response.encode().expect("可编码");
            let decoded = AdminResponse::decode(&encoded).expect("可解码");
            assert_eq!(decoded, response, "{method:?}");
        }
        assert_eq!(RequestId::NIL.len(), 36);
    }

    #[test]
    fn provider_keystore_refs_stay_within_the_label_budget() {
        // 最长 providerId 与最长字段名：标签必须仍在 keystore 的 128 字节上限内。
        let provider_id = "p".repeat(64);
        let field = "f".repeat(64);
        let reference = provider_keystore_ref(&provider_id, u64::from(u32::MAX));
        let label = provider_secret_label(&reference, &field);
        assert_eq!(reference.split('@').next().expect("摘要").len(), 16);
        assert!(reference.ends_with("@v4294967295"));
        assert!(label.len() <= 128, "标签长度 {} 超出上限", label.len());
        assert_eq!(label, format!("{reference}.{field}"));
        // 摘要对同一 provider 稳定、对不同 provider 与不同版本不同。
        assert_eq!(
            provider_keystore_ref("openai.primary", 1),
            provider_keystore_ref("openai.primary", 1)
        );
        assert_ne!(
            provider_keystore_ref("openai.primary", 1),
            provider_keystore_ref("openai.secondary", 1)
        );
        assert_ne!(
            provider_keystore_ref("openai.primary", 1),
            provider_keystore_ref("openai.primary", 2)
        );
        assert_eq!(sha256_hex_prefix("").len(), 16);
    }

    // -----------------------------------------------------------------------------------------
    // 设备/节点配对与信任（§5.3/§5.4、[R42]–[R48]）
    // -----------------------------------------------------------------------------------------

    use acp_core::model::{
        DeviceId, DeviceRecord, DeviceState, NodeId, NodeRecord, NodeState, PairingPeer,
        PeerIdentity,
    };

    use crate::local_admin::pairing::PAIRING_WINDOW_MS;
    use crate::local_admin::test_support::{TEST_PUBLIC_ORIGIN, test_nonce, test_public_key};

    const DEVICE_ID: &str = "2ae1c07c-0000-4000-8000-0000000000a1";
    const NODE_ID: &str = "2ae1c07c-0000-4000-8000-0000000000b1";
    const OWNER_ENDPOINT: &str = "wss://work-pc.example.test/node-link/v1";

    /// 一个设备的 claim 载荷（绑定必须逐字回显登记值）。
    fn device_peer(display_name: &str) -> PairingPeer {
        PairingPeer::try_new(
            PeerIdentity::Device(DeviceId::new(DEVICE_ID).expect("device id")),
            display_name,
            test_public_key(),
            TEST_PUBLIC_ORIGIN,
            test_nonce(),
        )
        .expect("claim 载荷合法")
    }

    /// 一个 Access Node 的 claim 载荷（节点配对的绑定是 endpoint）。
    fn node_peer(display_name: &str) -> PairingPeer {
        PairingPeer::try_new(
            PeerIdentity::Node(NodeId::new(NODE_ID).expect("node id")),
            display_name,
            test_public_key(),
            OWNER_ENDPOINT,
            test_nonce(),
        )
        .expect("claim 载荷合法")
    }

    /// 解出二维码 URL 里的无填充 base64url JSON。
    fn qr_json(url: &str, prefix: &str) -> Vec<u8> {
        let data = url.strip_prefix(prefix).expect("URL 形状与协议一致");
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(data)
            .expect("fragment 无填充 base64url")
    }

    /// 某个 secret 的持久摘要（`PairingRecord::secret_digest` 的口径）。
    fn secret_digest(secret: &[u8; 32]) -> String {
        use sha2::Digest as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(secret))
    }

    #[tokio::test]
    async fn device_pairing_full_flow_reaches_an_active_device() {
        let world = TestWorld::new();
        let router = world.router();

        // begin：二维码 URL 里的 payload 能被 sync-protocol 反序列化，且携带的 secret 就是持久摘要的来源。
        let begin = result_of(
            &router
                .handle(request(
                    Method::DevicePairBegin,
                    json!({
                        "requestedPacks": ["pack.observe", "pack.approve"],
                        "requestedScopes": ["command.status"],
                        "expiresInMs": null,
                    }),
                ))
                .await,
        );
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        assert_eq!(pairing_id.len(), 36);
        assert_eq!(begin["expiresAt"], json!("2026-09-18T09:17:03.412Z"));
        assert_eq!(
            begin["scopes"],
            json!([
                "command.status",
                "permission.resolve",
                "session.config.list",
                "session.list",
                "session.mode.list",
                "session.read",
            ]),
            "pack.* 必须展开成独立命令 scope"
        );
        let url = begin["pairingUrl"].as_str().expect("pairingUrl").to_owned();
        let payload: sync_protocol::pairing::QrPayload =
            serde_json::from_slice(&qr_json(&url, "https://work-pc.example.test/pair#data="))
                .expect("sync 侧可反序列化");
        assert_eq!(payload.pairing_id.as_str(), pairing_id);
        assert_eq!(payload.expires_at.as_str(), "2026-09-18T09:17:03.412Z");
        assert_eq!(payload.canonical_origin.as_str(), TEST_PUBLIC_ORIGIN);
        assert_eq!(
            payload.host_public_key.as_bytes(),
            test_public_key().as_bytes()
        );
        let registered = world.trust.pairing(&pairing_id).expect("配对已持久化");
        assert_eq!(
            registered.secret_digest().as_str(),
            secret_digest(payload.pairing_secret.as_bytes()),
            "URL 里的 secret 必须与落库摘要一致"
        );
        assert_eq!(registered.state(), PairingState::Created);
        assert_eq!(registered.target(), PairingTarget::Device);

        // claim 之前：只暴露 state 与 expiresAt（5.3）。
        let status = result_of(
            &router
                .handle(request(
                    Method::DevicePairStatus,
                    json!({"pairingId": pairing_id}),
                ))
                .await,
        );
        assert_eq!(status["state"], json!("pending"));
        assert_eq!(status["expiresAt"], json!("2026-09-18T09:17:03.412Z"));
        for field in [
            "displayName",
            "publicKeyFingerprint",
            "sas",
            "requestedScopes",
            "deviceId",
        ] {
            assert_eq!(status[field], json!(null), "claim 前 `{field}` 必须为 null");
        }
        assert_eq!(status.len(), 7, "§5.3 的七个字段逐项存在");

        // 模拟设备端 claim（HTTPS 路径属后续切片；这里只提交存储层同款事实）。
        world
            .claim(
                &PairingId::new(&pairing_id).expect("pairing id"),
                device_peer("Zhang's Phone"),
            )
            .await
            .expect("claim 提交成功");

        let status = result_of(
            &router
                .handle(request(
                    Method::DevicePairStatus,
                    json!({"pairingId": pairing_id}),
                ))
                .await,
        );
        assert_eq!(status["state"], json!("claimed"));
        assert_eq!(status["displayName"], json!("Zhang's Phone"));
        assert_eq!(
            status["publicKeyFingerprint"],
            json!(test_public_key().fingerprint().as_str())
        );
        let sas = status["sas"].as_str().expect("SAS 必须是字符串");
        assert_eq!(sas.len(), 6, "SAS 是 6 位十进制：{sas}");
        assert!(sas.bytes().all(|byte| byte.is_ascii_digit()), "{sas}");
        assert_eq!(status["deviceId"], json!(DEVICE_ID));
        assert_eq!(status["requestedScopes"], begin["scopes"]);

        // confirm：以用户确认的最终集合（请求集合的子集）创建 active 设备。
        let confirmed = result_of(
            &router
                .handle(request(
                    Method::DevicePairConfirm,
                    json!({"pairingId": pairing_id, "scopes": ["session.read"]}),
                ))
                .await,
        );
        assert_eq!(confirmed["deviceId"], json!(DEVICE_ID));
        assert_eq!(confirmed["scopes"], json!(["session.read"]));
        assert_eq!(confirmed["confirmedAt"], json!(world.clock_text()));
        let device = world.trust.device(DEVICE_ID).expect("设备记录已创建");
        assert_eq!(device.state(), DeviceState::Active);
        assert_eq!(
            device.scopes().iter().collect::<Vec<_>>(),
            vec!["session.read"]
        );

        // 已在内存里标记为已批准（secret 仍在；消费发生在首次 WSS 认证时）。
        assert!(
            world
                .authority
                .has_secret(&PairingId::new(&pairing_id).expect("pairing id")),
            "批准后的配对仍必须保留到首次认证"
        );

        // device.list 包含刚创建的记录。
        let listed = result_of(&router.handle(request(Method::DeviceList, json!({}))).await);
        assert_eq!(listed["devices"].as_array().expect("数组").len(), 1);
        assert_eq!(listed["devices"][0]["deviceId"], json!(DEVICE_ID));
        assert_eq!(listed["devices"][0]["state"], json!("active"));
        assert_eq!(listed["devices"][0]["scopes"], json!(["session.read"]));
        assert_eq!(listed["devices"][0]["createdAt"], json!(world.clock_text()));
        assert_eq!(listed["devices"][0]["lastSeenAt"], json!(null));
        assert_eq!(listed["devices"][0]["revokedAt"], json!(null));

        // 重复确认：状态已是 `approved`，与 5.3 的词表不符 → local.conflict。
        let (code, message) = error_of(
            &router
                .handle(request(
                    Method::DevicePairConfirm,
                    json!({"pairingId": pairing_id, "scopes": []}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Conflict);
        assert!(
            !message.contains("data="),
            "错误消息不得回显 URL：{message}"
        );
    }

    #[tokio::test]
    async fn expired_and_rejected_pairings_never_create_trust() {
        let world = TestWorld::new();
        let router = world.router();

        // 过期（时钟判定）：status 呈现 `expired`，confirm 回 `local.expired`，不创建信任。
        let begin = result_of(
            &router
                .handle(request(
                    Method::DevicePairBegin,
                    json!({
                        "requestedPacks": [],
                        "requestedScopes": ["session.read"],
                        "expiresInMs": 1000,
                    }),
                ))
                .await,
        );
        let expired_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        assert_eq!(begin["expiresAt"], json!("2026-09-18T09:12:04.412Z"));
        world.clock.set("2026-09-18T09:12:05.500Z");
        let status = result_of(
            &router
                .handle(request(
                    Method::DevicePairStatus,
                    json!({"pairingId": expired_id}),
                ))
                .await,
        );
        assert_eq!(status["state"], json!("expired"));
        assert_eq!(status["sas"], json!(null));
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::DevicePairConfirm,
                    json!({"pairingId": expired_id, "scopes": []}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Expired);
        assert_eq!(world.trust.device_count(), 0, "过期不创建信任");

        // 拒绝：`{}`，之后 confirm 回 `local.expired`，仍然没有信任记录。
        world.clock.set("2026-09-18T09:12:03.412Z");
        let begin = result_of(
            &router
                .handle(request(
                    Method::DevicePairBegin,
                    json!({
                        "requestedPacks": [],
                        "requestedScopes": ["session.read"],
                        "expiresInMs": null,
                    }),
                ))
                .await,
        );
        let rejected_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        world.clock.set("2026-09-18T09:12:03.412Z");
        // 尚未认领时 confirm：状态不允许，但既非过期也非拒绝 → local.conflict。
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::DevicePairConfirm,
                    json!({"pairingId": rejected_id, "scopes": []}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Conflict);

        world
            .claim(
                &PairingId::new(&rejected_id).expect("pairing id"),
                device_peer("Zhang's Phone"),
            )
            .await
            .expect("claim 提交成功");
        let rejected = result_of(
            &router
                .handle(request(
                    Method::DevicePairReject,
                    json!({"pairingId": rejected_id, "reason": "user said no"}),
                ))
                .await,
        );
        assert_eq!(rejected, JsonObject::new(), "reject 的 result 是 {{}}");
        assert_eq!(
            world.trust.pairing(&rejected_id).expect("配对").state(),
            PairingState::Rejected
        );
        for (method, params) in [
            (
                Method::DevicePairConfirm,
                json!({"pairingId": rejected_id, "scopes": []}),
            ),
            (
                Method::DevicePairReject,
                json!({"pairingId": rejected_id, "reason": null}),
            ),
        ] {
            let (code, message) = error_of(&router.handle(request(method, params)).await);
            assert_eq!(code, LocalErrorCode::Expired, "{method:?}");
            assert!(message.contains("expired or was rejected"), "{message}");
        }
        assert_eq!(world.trust.device_count(), 0, "拒绝不创建信任");
        let listed = result_of(&router.handle(request(Method::DeviceList, json!({}))).await);
        assert_eq!(listed["devices"], json!([]));
    }

    #[tokio::test]
    async fn confirm_rejects_sets_beyond_the_requested_ones() {
        let world = TestWorld::new();
        let router = world.router();
        let begin = result_of(
            &router
                .handle(request(
                    Method::DevicePairBegin,
                    json!({
                        "requestedPacks": [],
                        "requestedScopes": ["session.read"],
                        "expiresInMs": null,
                    }),
                ))
                .await,
        );
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        world
            .claim(
                &PairingId::new(&pairing_id).expect("pairing id"),
                device_peer("Zhang's Phone"),
            )
            .await
            .expect("claim 提交成功");

        let (code, message) = error_of(
            &router
                .handle(request(
                    Method::DevicePairConfirm,
                    json!({"pairingId": pairing_id, "scopes": ["session.read", "session.prompt"]}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
        assert!(message.contains("must not exceed"), "{message}");
        assert_eq!(world.trust.device_count(), 0, "越界不得创建信任");
        assert_eq!(
            world.trust.pairing(&pairing_id).expect("配对").state(),
            PairingState::PendingConfirmation,
            "越界不得推进状态"
        );
    }

    #[tokio::test]
    async fn revoke_closes_the_connection_after_the_commit_and_list_reflects_it() {
        let world = TestWorld::new();
        let router = world.router();
        let pairing_id = activate_device(&world, &router).await;
        assert!(world.closer.closed_devices().is_empty());

        let revoked = result_of(
            &router
                .handle(request(
                    Method::DeviceRevoke,
                    json!({"deviceId": DEVICE_ID}),
                ))
                .await,
        );
        assert_eq!(revoked["deviceId"], json!(DEVICE_ID));
        assert_eq!(revoked["revokedAt"], json!(world.clock_text()));
        assert_eq!(
            world.closer.closed_devices(),
            vec![DEVICE_ID.to_owned()],
            "提交后必须关闭该设备的 active connection"
        );
        let listed = result_of(&router.handle(request(Method::DeviceList, json!({}))).await);
        assert_eq!(listed["devices"][0]["state"], json!("revoked"));
        assert_eq!(listed["devices"][0]["revokedAt"], json!(world.clock_text()));
        // 未知设备：local.not_found，且不触发关闭。
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::DeviceRevoke,
                    json!({"deviceId": "2ae1c07c-0000-4000-8000-0000000000ff"}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
        assert_eq!(world.closer.closed_devices().len(), 1);
        assert_eq!(
            world.trust.pairing(&pairing_id).expect("配对").state(),
            PairingState::Approved
        );
    }

    /// §7：`*.revoke` 的重试得到 `local.not_found`。存储层的撤销是幂等的，因此该判定必须在调用
    /// 撤销用例之前做出；否则重试会以「成功」返回（本用例在把 `FakeExports` 改成与存储同款幂等
    /// 后先红后绿，`FakeTrust` 的撤销本就照抄了存储语义）。
    #[tokio::test]
    async fn device_revoke_retry_and_unknown_id_report_not_found() {
        let world = TestWorld::new();
        let router = world.router();
        activate_device(&world, &router).await;

        // ① 首次撤销成功：返回持久化的撤销时间。
        let first_at = world.clock_text();
        let revoked = result_of(
            &router
                .handle(request(
                    Method::DeviceRevoke,
                    json!({"deviceId": DEVICE_ID}),
                ))
                .await,
        );
        assert_eq!(revoked["deviceId"], json!(DEVICE_ID));
        assert_eq!(revoked["revokedAt"], json!(first_at));
        assert_eq!(world.closer.closed_devices(), vec![DEVICE_ID.to_owned()]);

        // ② 重试同一撤销 → `local.not_found`，且不再触发一次关闭。
        world.clock.set("2026-09-18T11:00:00.000Z");
        let (code, message) = error_of(
            &router
                .handle(request(
                    Method::DeviceRevoke,
                    json!({"deviceId": DEVICE_ID}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
        assert!(message.contains(DEVICE_ID), "{message}");
        assert_eq!(
            world.closer.closed_devices(),
            vec![DEVICE_ID.to_owned()],
            "重试不得再走一次撤销与关闭"
        );
        assert_eq!(
            world.trust.device(DEVICE_ID).expect("设备").revoked_at(),
            Some(&Timestamp::new(&first_at).expect("timestamp")),
            "重试不得改写首次撤销时间"
        );

        // ③ 未知 id → `local.not_found`，同样不触发关闭。
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::DeviceRevoke,
                    json!({"deviceId": "2ae1c07c-0000-4000-8000-0000000000f1"}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
        assert_eq!(world.closer.closed_devices().len(), 1);
    }

    /// §7：`node.revoke` 与 `device.revoke` 同款的重试语义（两种角色行同一事务进入 `revoked`）。
    #[tokio::test]
    async fn node_revoke_retry_and_unknown_id_report_not_found() {
        let world = TestWorld::new();
        let router = world.router();
        let pairing_id = begin_and_claim_node(&world, &router).await;
        result_of(
            &router
                .handle(request(
                    Method::NodePairConfirm,
                    json!({"pairingId": pairing_id, "grants": ["grant.observe"]}),
                ))
                .await,
        );

        // ① 首次撤销成功：返回持久化的撤销时间。
        let first_at = world.clock_text();
        let revoked = result_of(
            &router
                .handle(request(Method::NodeRevoke, json!({"nodeId": NODE_ID})))
                .await,
        );
        assert_eq!(revoked["nodeId"], json!(NODE_ID));
        assert_eq!(revoked["revokedAt"], json!(first_at));
        assert_eq!(world.closer.closed_nodes(), vec![NODE_ID.to_owned()]);

        // ② 重试同一撤销 → `local.not_found`，且不再触发一次关闭。
        world.clock.set("2026-09-18T11:00:00.000Z");
        let (code, message) = error_of(
            &router
                .handle(request(Method::NodeRevoke, json!({"nodeId": NODE_ID})))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
        assert!(message.contains(NODE_ID), "{message}");
        assert_eq!(
            world.closer.closed_nodes(),
            vec![NODE_ID.to_owned()],
            "重试不得再走一次撤销与关闭"
        );
        assert_eq!(
            world.trust.nodes_of(NODE_ID)[0].revoked_at(),
            Some(&Timestamp::new(&first_at).expect("timestamp")),
            "重试不得改写首次撤销时间"
        );

        // ③ 未知 id → `local.not_found`，同样不触发关闭。
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::NodeRevoke,
                    json!({"nodeId": "2ae1c07c-0000-4000-8000-0000000000f2"}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
        assert_eq!(world.closer.closed_nodes().len(), 1);
    }

    #[tokio::test]
    async fn a_failed_settlement_keeps_the_memory_state_strict() {
        let world = TestWorld::new();
        let router = world.router();
        let pairing_id = begin_and_claim_device(&world, &router, &["session.read"]).await;
        let pairing = PairingId::new(&pairing_id).expect("pairing id");

        world.trust.fail_next_settle(PortError::Unavailable(
            acp_core::model::UnavailableKind::IoError,
        ));
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::DevicePairConfirm,
                    json!({"pairingId": pairing_id, "scopes": ["session.read"]}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Unavailable);
        assert_eq!(world.trust.device_count(), 0, "写集失败不得创建信任");
        assert_eq!(
            world.trust.pairing(&pairing_id).expect("配对").state(),
            PairingState::PendingConfirmation,
            "写集失败不得推进配对状态"
        );
        // 内存 secret 必须保留（消费只发生在已批准后的首次认证）。
        assert!(world.authority.has_secret(&pairing));

        // 重试仍然成立：说明内存态没有跨过持久状态（`Authority` 不提供「已批准」读口，因此这里用
        // 「重试成功 + 库中无提前信任」作为可核对的证据）。
        let confirmed = result_of(
            &router
                .handle(request(
                    Method::DevicePairConfirm,
                    json!({"pairingId": pairing_id, "scopes": ["session.read"]}),
                ))
                .await,
        );
        assert_eq!(confirmed["deviceId"], json!(DEVICE_ID));
        assert_eq!(world.trust.device_count(), 1);
    }

    #[tokio::test]
    async fn node_owner_pairing_flow_pairs_a_node_with_initial_grants() {
        let world = TestWorld::new();
        let router = world.router();

        let begin = result_of(
            &router
                .handle(request(
                    Method::NodePairBegin,
                    json!({
                        "mode": "owner",
                        "pairingUrl": null,
                        "displayName": "Owner Node",
                        "requestedGrants": ["grant.observe", "grant.remote-work"],
                    }),
                ))
                .await,
        );
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        assert_eq!(begin["state"], json!("pending"));
        assert_eq!(begin["expiresAt"], json!("2026-09-18T09:17:03.412Z"));
        assert_eq!(begin["sas"], json!(null));
        assert_eq!(begin["peerNodeId"], json!(null));
        assert_eq!(begin["peerPublicKeyFingerprint"], json!(null));
        let url = begin["pairingUrl"].as_str().expect("pairingUrl").to_owned();
        let payload: node_link_protocol::pairing::QrPayload = serde_json::from_slice(&qr_json(
            &url,
            "https://work-pc.example.test/node-link/pair#data=",
        ))
        .expect("Node Link 侧可反序列化");
        assert_eq!(payload.endpoint.as_str(), OWNER_ENDPOINT);
        assert_eq!(payload.pairing_id.as_str(), pairing_id);
        assert_eq!(
            payload.owner_public_key.as_bytes(),
            test_public_key().as_bytes()
        );
        let registered = world.trust.pairing(&pairing_id).expect("配对已持久化");
        assert_eq!(registered.target(), PairingTarget::Node);
        assert_eq!(
            registered.secret_digest().as_str(),
            secret_digest(payload.pairing_secret.as_bytes())
        );
        assert_eq!(registered.host_binding(), OWNER_ENDPOINT);
        assert_eq!(registered.display_name(), Some("Owner Node"));

        // claim 之前：除 state 与 expiresAt 外全为 null，`lastRefreshError` 恒 null（无远程刷新）。
        let status = result_of(
            &router
                .handle(request(
                    Method::NodePairStatus,
                    json!({"pairingId": pairing_id}),
                ))
                .await,
        );
        assert_eq!(status["state"], json!("pending"));
        assert_eq!(status["lastRefreshError"], json!(null));
        for field in [
            "peerNodeId",
            "peerPublicKeyFingerprint",
            "sas",
            "requestedGrants",
        ] {
            assert_eq!(status[field], json!(null), "claim 前 `{field}` 必须为 null");
        }

        world
            .claim(
                &PairingId::new(&pairing_id).expect("pairing id"),
                node_peer("Access Node"),
            )
            .await
            .expect("claim 提交成功");
        let status = result_of(
            &router
                .handle(request(
                    Method::NodePairStatus,
                    json!({"pairingId": pairing_id}),
                ))
                .await,
        );
        assert_eq!(status["state"], json!("pending_confirmation"));
        assert_eq!(status["peerNodeId"], json!(NODE_ID));
        assert_eq!(
            status["peerPublicKeyFingerprint"],
            json!(test_public_key().fingerprint().as_str())
        );
        assert_eq!(status["sas"].as_str().expect("SAS").len(), 6);
        assert_eq!(
            status["requestedGrants"],
            json!(["grant.observe", "grant.remote-work"])
        );
        assert_eq!(status["lastRefreshError"], json!(null));

        let confirmed = result_of(
            &router
                .handle(request(
                    Method::NodePairConfirm,
                    json!({"pairingId": pairing_id, "grants": ["grant.observe"]}),
                ))
                .await,
        );
        assert_eq!(confirmed["nodeId"], json!(NODE_ID));
        assert_eq!(confirmed["grants"], json!(["grant.observe"]));
        assert_eq!(confirmed["confirmedAt"], json!(world.clock_text()));

        let listed = result_of(&router.handle(request(Method::NodeList, json!({}))).await);
        assert_eq!(listed["nodes"].as_array().expect("数组").len(), 1);
        assert_eq!(listed["nodes"][0]["nodeId"], json!(NODE_ID));
        assert_eq!(listed["nodes"][0]["kind"], json!("access"));
        assert_eq!(listed["nodes"][0]["state"], json!("paired"));
        assert_eq!(listed["nodes"][0]["grants"], json!(["grant.observe"]));
        assert_eq!(listed["nodes"][0]["ownerEndpoint"], json!(null));
        assert_eq!(listed["nodes"][0]["lastConnectedAt"], json!(null));
        assert_eq!(listed["nodes"][0]["displayName"], json!("Access Node"));
        let node = world.trust.nodes_of(NODE_ID);
        assert_eq!(node.len(), 1);
        assert_eq!(node[0].state(), NodeState::Paired);

        let revoked = result_of(
            &router
                .handle(request(Method::NodeRevoke, json!({"nodeId": NODE_ID})))
                .await,
        );
        assert_eq!(revoked["nodeId"], json!(NODE_ID));
        assert_eq!(revoked["revokedAt"], json!(world.clock_text()));
        assert_eq!(world.closer.closed_nodes(), vec![NODE_ID.to_owned()]);
        let listed = result_of(&router.handle(request(Method::NodeList, json!({}))).await);
        assert_eq!(listed["nodes"][0]["state"], json!("revoked"));
        assert_eq!(listed["nodes"][0]["revokedAt"], json!(world.clock_text()));
    }

    #[tokio::test]
    async fn node_access_mode_is_unsupported_and_creates_nothing() {
        let world = TestWorld::new();
        let router = world.router();
        let (code, message) = error_of(
            &router
                .handle(request(
                    Method::NodePairBegin,
                    json!({
                        "mode": "access",
                        "pairingUrl": "https://owner.example/node-link/pair#data=abc",
                        "displayName": "Access Node",
                        "requestedGrants": ["grant.observe"],
                    }),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Unsupported);
        assert!(message.contains("access"), "{message}");
        assert_eq!(world.trust.pairing_count(), 0, "不得创建配对行");
        assert_eq!(world.trust.node_count(), 0);
        assert!(
            !message.contains("data="),
            "错误消息不得回显 pairingUrl：{message}"
        );
        // 未知 mode 仍然是参数错误（只有明确写 `access` 才是「不支持」）。
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::NodePairBegin,
                    json!({
                        "mode": "peer",
                        "pairingUrl": null,
                        "displayName": "X",
                        "requestedGrants": [],
                    }),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
    }

    #[tokio::test]
    async fn pairing_methods_fail_closed_without_a_public_origin() {
        let world = TestWorld::with_public_origin(None);
        let router = world.router();
        for (method, params) in [
            (
                Method::DevicePairBegin,
                json!({
                    "requestedPacks": [],
                    "requestedScopes": ["session.read"],
                    "expiresInMs": null,
                }),
            ),
            (
                Method::NodePairBegin,
                json!({
                    "mode": "owner",
                    "pairingUrl": null,
                    "displayName": "Owner Node",
                    "requestedGrants": ["grant.observe"],
                }),
            ),
        ] {
            let (code, message) = error_of(&router.handle(request(method, params)).await);
            assert_eq!(code, LocalErrorCode::Unavailable, "{method:?}");
            assert!(message.contains("daemon.public_origin"), "{message}");
        }
        assert_eq!(world.trust.pairing_count(), 0, "失败关闭不得留下任何写入");
        assert_eq!(world.trust.device_count(), 0);
        assert_eq!(world.trust.node_count(), 0);
    }

    #[tokio::test]
    async fn unknown_and_family_mismatched_pairings_answer_not_found() {
        let world = TestWorld::new();
        let router = world.router();
        let unknown = "2ae1c07c-0000-4000-8000-0000000000ee";
        for (method, params) in [
            (Method::DevicePairStatus, json!({"pairingId": unknown})),
            (
                Method::DevicePairConfirm,
                json!({"pairingId": unknown, "scopes": []}),
            ),
            (
                Method::DevicePairReject,
                json!({"pairingId": unknown, "reason": null}),
            ),
            (Method::NodePairStatus, json!({"pairingId": unknown})),
        ] {
            let (code, _) = error_of(&router.handle(request(method, params)).await);
            assert_eq!(code, LocalErrorCode::NotFound, "{method:?}");
        }

        // 目标族不匹配：device.* 不服务节点配对，反之同理。
        let device_id = begin_and_claim_device(&world, &router, &["session.read"]).await;
        let node_id = begin_and_claim_node(&world, &router).await;
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::NodePairStatus,
                    json!({"pairingId": device_id}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::DevicePairStatus,
                    json!({"pairingId": node_id}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
    }

    #[tokio::test]
    async fn pairing_begin_validates_names_windows_and_shapes() {
        let world = TestWorld::new();
        let router = world.router();
        for rejected in [
            json!({"requestedPacks": ["pack.nope"], "requestedScopes": [], "expiresInMs": null}),
            json!({"requestedPacks": [], "requestedScopes": ["nope"], "expiresInMs": null}),
            json!({"requestedPacks": [], "requestedScopes": ["local.audit.export"], "expiresInMs": null}),
            json!({"requestedPacks": [], "requestedScopes": [], "expiresInMs": 0}),
            json!({"requestedPacks": [], "requestedScopes": [], "expiresInMs": PAIRING_WINDOW_MS + 1}),
            json!({"requestedPacks": [], "requestedScopes": [], "expiresInMs": -1}),
            json!({"requestedPacks": [], "requestedScopes": [], "expiresInMs": 1.5}),
            json!({"requestedPacks": "pack.observe", "requestedScopes": [], "expiresInMs": null}),
            json!({"requestedPacks": [], "requestedScopes": [], "expiresInMs": null, "grants": []}),
            json!({"requestedPacks": [], "requestedScopes": []}),
        ] {
            let (code, _) = error_of(
                &router
                    .handle(request(Method::DevicePairBegin, rejected.clone()))
                    .await,
            );
            assert_eq!(code, LocalErrorCode::InvalidParams, "{rejected}");
        }
        assert_eq!(world.trust.pairing_count(), 0, "校验失败不得写配对行");

        for rejected in [
            json!({
                "mode": "owner", "pairingUrl": "https://x/pair#data=a",
                "displayName": "N", "requestedGrants": [],
            }),
            json!({
                "mode": "owner", "pairingUrl": null,
                "displayName": "N", "requestedGrants": ["grant.nope"],
            }),
            json!({
                "mode": "owner", "pairingUrl": null,
                "displayName": "N", "requestedGrants": ["session.read"],
            }),
            json!({
                "mode": "owner", "pairingUrl": null,
                "displayName": "", "requestedGrants": [],
            }),
            json!({
                "mode": "owner", "pairingUrl": null,
                "displayName": "N", "requestedGrants": [], "scopes": [],
            }),
        ] {
            let (code, _) = error_of(
                &router
                    .handle(request(Method::NodePairBegin, rejected.clone()))
                    .await,
            );
            assert_eq!(code, LocalErrorCode::InvalidParams, "{rejected}");
        }
        assert_eq!(world.trust.pairing_count(), 0);

        // reject 的 reason 上界（≤ 256 字符）：超长属参数非法，且不改变状态。
        let pairing_id = begin_and_claim_device(&world, &router, &["session.read"]).await;
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::DevicePairReject,
                    json!({"pairingId": pairing_id, "reason": "x".repeat(257)}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
        assert_eq!(
            world.trust.pairing(&pairing_id).expect("配对").state(),
            PairingState::PendingConfirmation
        );
    }

    #[tokio::test]
    async fn node_confirm_and_reject_error_paths_answer_the_documented_codes() {
        let world = TestWorld::new();
        let router = world.router();
        let pairing_id = begin_and_claim_node(&world, &router).await;

        // 确认集合超出请求值 → 参数非法，不创建信任。
        let (code, message) = error_of(
            &router
                .handle(request(
                    Method::NodePairConfirm,
                    json!({"pairingId": pairing_id, "grants": ["grant.observe", "grant.remote-work"]}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
        assert!(message.contains("must not exceed"), "{message}");
        assert_eq!(world.trust.node_count(), 0);

        // 未认领的节点配对不能确认（状态不允许）。
        let pending = result_of(
            &router
                .handle(request(
                    Method::NodePairBegin,
                    json!({
                        "mode": "owner",
                        "pairingUrl": null,
                        "displayName": "Owner Node",
                        "requestedGrants": ["grant.observe"],
                    }),
                ))
                .await,
        );
        let unclaimed = pending["pairingId"].as_str().expect("pairingId").to_owned();
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::NodePairConfirm,
                    json!({"pairingId": unclaimed, "grants": []}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::Conflict);

        // 拒绝成功 → `{}`；再次拒绝与之后确认都回 `local.expired`。
        let rejected = result_of(
            &router
                .handle(request(
                    Method::NodePairReject,
                    json!({"pairingId": pairing_id, "reason": null}),
                ))
                .await,
        );
        assert_eq!(rejected, JsonObject::new());
        for (method, params) in [
            (
                Method::NodePairReject,
                json!({"pairingId": pairing_id, "reason": null}),
            ),
            (
                Method::NodePairConfirm,
                json!({"pairingId": pairing_id, "grants": []}),
            ),
        ] {
            let (code, _) = error_of(&router.handle(request(method, params)).await);
            assert_eq!(code, LocalErrorCode::Expired, "{method:?}");
        }
        assert_eq!(world.trust.node_count(), 0, "拒绝不创建信任");
        let listed = result_of(&router.handle(request(Method::NodeList, json!({}))).await);
        assert_eq!(listed["nodes"], json!([]));

        // 未知节点撤销 → `local.not_found`，且不触发关闭。
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::NodeRevoke,
                    json!({"nodeId": "2ae1c07c-0000-4000-8000-0000000000ff"}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);
        assert!(world.closer.closed_nodes().is_empty());
    }

    #[tokio::test]
    async fn list_methods_include_pending_and_revoked_records() {
        let world = TestWorld::new();
        let router = world.router();
        let pending_device = DeviceRecord::try_new(
            DeviceId::new("2ae1c07c-0000-4000-8000-0000000000c1").expect("device id"),
            "Pending Phone",
            test_public_key().fingerprint(),
            ScopeSet::empty(),
            DeviceState::Pending,
            Timestamp::new("2026-09-18T09:00:00.000Z").expect("timestamp"),
            None,
            None,
        )
        .expect("pending 设备记录合法");
        world.trust.seed_device(pending_device);
        let pending_node = NodeRecord::try_new(
            NodeId::new("2ae1c07c-0000-4000-8000-0000000000c2").expect("node id"),
            "Pending Node",
            NodeKind::Access,
            test_public_key().fingerprint(),
            GrantSet::empty(),
            NodeState::Pending,
            None,
            Timestamp::new("2026-09-18T09:00:00.000Z").expect("timestamp"),
            None,
            None,
        )
        .expect("pending 节点记录合法");
        world.trust.seed_node(pending_node);

        // 再走一遍完整流程（active → revoked），使两个族各有 pending 与 revoked 两条。
        let device_pairing = activate_device(&world, &router).await;
        result_of(
            &router
                .handle(request(
                    Method::DeviceRevoke,
                    json!({"deviceId": DEVICE_ID}),
                ))
                .await,
        );
        assert_eq!(
            world.trust.pairing(&device_pairing).expect("配对").state(),
            PairingState::Approved
        );
        let node_pairing = begin_and_claim_node(&world, &router).await;
        result_of(
            &router
                .handle(request(
                    Method::NodePairConfirm,
                    json!({"pairingId": node_pairing, "grants": ["grant.observe"]}),
                ))
                .await,
        );
        result_of(
            &router
                .handle(request(Method::NodeRevoke, json!({"nodeId": NODE_ID})))
                .await,
        );

        let listed = result_of(&router.handle(request(Method::DeviceList, json!({}))).await);
        let states: Vec<&str> = listed["devices"]
            .as_array()
            .expect("数组")
            .iter()
            .map(|record| record["state"].as_str().expect("state"))
            .collect();
        assert_eq!(states, vec!["revoked", "pending"], "含 pending 与 revoked");
        let listed = result_of(&router.handle(request(Method::NodeList, json!({}))).await);
        let states: Vec<&str> = listed["nodes"]
            .as_array()
            .expect("数组")
            .iter()
            .map(|record| record["state"].as_str().expect("state"))
            .collect();
        assert_eq!(states, vec!["revoked", "pending"]);
        assert_eq!(world.trust.device_count(), 2);
        assert_eq!(world.trust.node_count(), 2);
    }

    #[tokio::test]
    async fn list_methods_reject_unknown_parameters() {
        let world = TestWorld::new();
        let router = world.router();
        for method in [Method::DeviceList, Method::NodeList] {
            let (code, message) =
                error_of(&router.handle(request(method, json!({"unknown": 1}))).await);
            assert_eq!(code, LocalErrorCode::InvalidParams, "{method:?}");
            assert!(message.contains("unknown"), "{message}");
        }
        // `device.revoke`/`node.revoke` 的 id 形状也走同一条参数校验（`device.revoke` 的未知设备
        // 已在撤销用例里覆盖）。
        let (code, _) = error_of(
            &router
                .handle(request(Method::NodeRevoke, json!({"nodeId": "not-a-uuid"})))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
        let (code, _) = error_of(
            &router
                .handle(request(Method::DeviceRevoke, json!({"deviceId": null})))
                .await,
        );
        assert_eq!(code, LocalErrorCode::InvalidParams);
    }

    /// 走完 begin → claim，得到一个等待本地确认的设备配对（返回 `pairingId`）。
    async fn begin_and_claim_device(
        world: &TestWorld,
        router: &LocalAdminRouter,
        scopes: &[&str],
    ) -> String {
        let begin = result_of(
            &router
                .handle(request(
                    Method::DevicePairBegin,
                    json!({
                        "requestedPacks": [],
                        "requestedScopes": scopes,
                        "expiresInMs": null,
                    }),
                ))
                .await,
        );
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        world
            .claim(
                &PairingId::new(&pairing_id).expect("pairing id"),
                device_peer("Zhang's Phone"),
            )
            .await
            .expect("claim 提交成功");
        assert_eq!(
            world.trust.pairing(&pairing_id).expect("配对").state(),
            PairingState::PendingConfirmation
        );
        pairing_id
    }

    /// 走完 begin → claim → confirm，得到一个 active 设备（返回 `pairingId`）。
    async fn activate_device(world: &TestWorld, router: &LocalAdminRouter) -> String {
        let pairing_id = begin_and_claim_device(world, router, &["session.read"]).await;
        result_of(
            &router
                .handle(request(
                    Method::DevicePairConfirm,
                    json!({"pairingId": pairing_id, "scopes": ["session.read"]}),
                ))
                .await,
        );
        assert_eq!(
            world.trust.device(DEVICE_ID).expect("设备").state(),
            DeviceState::Active
        );
        pairing_id
    }

    /// 走完 begin → claim，得到一个等待本地确认的节点配对。
    async fn begin_and_claim_node(world: &TestWorld, router: &LocalAdminRouter) -> String {
        let begin = result_of(
            &router
                .handle(request(
                    Method::NodePairBegin,
                    json!({
                        "mode": "owner",
                        "pairingUrl": null,
                        "displayName": "Owner Node",
                        "requestedGrants": ["grant.observe"],
                    }),
                ))
                .await,
        );
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        world
            .claim(
                &PairingId::new(&pairing_id).expect("pairing id"),
                node_peer("Access Node"),
            )
            .await
            .expect("claim 提交成功");
        pairing_id
    }
}
