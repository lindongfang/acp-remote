//! 方法路由：把一条已通过信封校验的管理请求分派到 §5 的方法实现
//! （`docs/LOCAL_ADMIN_PROTOCOL.md` §5、`docs/MODULE_ARCHITECTURE.md` §4.9、design.md 决策 6）。
//!
//! 边界纪律：
//!
//! - 业务方法只调用 `core::use_cases`，且恒以 `Actor::LocalCli` 调用（`require_local` 已就位）：
//!   server 侧不做第二套授权判定，也不直接查询 SQLite；
//! - `daemon.status`/`daemon.stop` 由组合根经注入的 [`DaemonControl`] 回答（server 不依赖 `app`）；
//! - `provider.configure` 的凭据值只经 `identity_auth::IdentityKeystore` 端口写入，值只以
//!   [`SecretBytes`] 形态流转（不进日志、不进错误、不进 `result`）；
//! - `audit.export` 直接用注入的 `Arc<dyn AuditStore>`（§5.1：不经业务用例）；
//! - 本模块不写审计：撤销类与 `provider.configure` 的审计由 core 的写集在同一事务里提交；
//!   组合根层面的拒绝连接审计由 WP3a 的 `AuditHook` 承担。§14.2 没有「方法失败」这一类类别，
//!   因此路由层失败只写结构化日志（method + 错误码，不含参数），不伪造审计事件。

use std::sync::Arc;

use acp_core::model::{Actor, AgentProfile, ProviderRef, WorkspaceRecord};
use acp_core::ports::{AuditQuery, AuditStore, Clock};
use acp_core::use_cases::UseCases;
use identity_auth::{IdentityKeystore, KeystoreError, SecretBytes, SecretPurpose};
use serde_json::Value;

use crate::local_admin::audit;
use crate::local_admin::daemon::DaemonControl;
use crate::local_admin::envelope::{AdminRequest, AdminResponse, JsonObject};
use crate::local_admin::error::{AdminError, LocalErrorCode};
use crate::local_admin::handler::LocalAdminHandler;
use crate::local_admin::method::Method;
use crate::local_admin::params::{self, ProviderConfigure};
use crate::local_admin::view::{self, object, text, timestamp};

/// [`LocalAdminRouter`] 的组合根注入口。
pub struct LocalAdminDeps {
    /// Daemon 生命周期与运行期状态（由 `app::daemon` 实现）。
    pub daemon: Arc<dyn DaemonControl>,
    /// 业务用例面（本地配置族、Export/Import 族）。
    pub core: Arc<UseCases>,
    /// Provider 凭据的平台安全存储端口（由组合根注入 `identity-keystore` 的实现）。
    pub keystore: Arc<dyn IdentityKeystore>,
    /// 审计仓储的**读取**端口（§5.1：`audit.export` 不经业务用例）。
    pub audit: Arc<dyn AuditStore>,
    /// 时间来源（core 不读系统时间：写集的时间戳由适配器经本端口取得后传入）。
    pub clock: Arc<dyn Clock>,
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
            // §5.7：本方法只登记名字，`params`/`result` 与 `NODE_LINK_PROTOCOL.md` §12.3 的
            // `node.rotate-key.request`/`node.rotate-key.result` 同批定义；字段定义落地前调用恒回
            // `local.unsupported`，不自行填充参数形状。
            Method::NodeRotateKeyBegin => {
                Err(AdminError::unsupported_method(Method::NodeRotateKeyBegin))
            }
            // §5.3/§5.4 的设备与节点配对族尚未实现（WP3b2）：
            // 集内未实现的方法回 `local.unsupported`（§6），连接保持可用。
            unimplemented => Err(AdminError::unsupported_method(unimplemented)),
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

    async fn export_revoke(&self, params: &JsonObject) -> Result<JsonObject, AdminError> {
        let actor = Actor::LocalCli;
        let export_id = params::export_revoke(params)?;
        self.deps
            .core
            .revoke_export(&actor, &export_id)
            .await
            .map_err(|error| params::map_port_error("export.revoke", error))?;
        // §5.5：必须在持久状态提交后才返回。撤销时间取读回的持久值，不由本层时钟猜。
        let record = self
            .deps
            .core
            .export(&actor, &export_id)
            .await
            .map_err(|error| params::map_port_error("export.revoke", error))?;
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

        // 重试同一个撤销 → local.not_found（§7）。
        let (code, _) = error_of(
            &router
                .handle(request(
                    Method::ExportRevoke,
                    json!({"exportId": "export-1"}),
                ))
                .await,
        );
        assert_eq!(code, LocalErrorCode::NotFound);

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
        for method in [
            Method::DevicePairBegin,
            Method::DeviceList,
            Method::DeviceRevoke,
            Method::NodePairBegin,
            Method::NodeList,
            Method::NodeRevoke,
            // §5.7：字段定义落地前 `node.rotate-key.begin` 也走这一条。
            Method::NodeRotateKeyBegin,
        ] {
            let (code, message) = error_of(&router.handle(request(method, json!({}))).await);
            assert_eq!(code, LocalErrorCode::Unsupported, "{method:?}");
            assert!(message.contains(method.as_str()), "{message}");
        }
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
}
