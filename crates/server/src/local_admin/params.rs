//! 方法参数校验与 `PortError` → `local.*` 映射
//! （`docs/LOCAL_ADMIN_PROTOCOL.md` §1.1、§5.2–§5.6、§6）。
//!
//! 本模块是路由的**唯一**参数形状知识所在：`params` 与嵌套对象都是 closed object（§1.1），
//! 缺字段、类型不符或出现未知字段一律 `local.invalid_params`（§4 规则 4）。校验函数是纯函数：
//! 不接触存储、时钟、keystore 与日志；只有 §5.2 的 `rootPath` 与 §5.6 的 `outputPath` 需要按合同
//! 判定文件系统事实（已存在的绝对目录 / 绝对路径），那是可判定规则本身的一部分。
//!
//! 两条口径（在报告里登记）：
//!
//! - 错误消息里只回显**标识符类**的对端输入（参数名、方法名、audit 类别名），不回显自由文本值
//!   （路径、命令、显示名、凭据值）——§4 规则 6 禁止 secret、堆栈与完整敏感路径。
//! - 语义级词表（环境变量名与其保留前缀、`grant.*`、时间戳格式）**只有 core 一份定义**：本层只做
//!   类型与结构校验，语义拒绝由 core 的构造器返回 `InvalidRequest`，再由 [`map_port_error`] 收敛成
//!   `local.invalid_params`，避免出现第二份词表。

use std::path::{Component, Path, PathBuf};

use acp_core::model::{
    AgentId, AuditAction, CachePolicy, ConflictKind, DeviceId, ExportId, ExportRecord,
    ExportTemplate, GrantSet, ImportId, NodeId, PairingId, PairingRecord, PairingState, PortError,
    ProviderRefKind, ScopeSet, TemplateId, Timestamp, WorkspaceAlias, WorkspaceAliasEntry,
};
use identity_auth::{PairingError, authorization};
use serde_json::Value;

use crate::local_admin::daemon::MAX_STOP_GRACE_MS;
use crate::local_admin::envelope::JsonObject;
use crate::local_admin::error::{AdminError, LocalErrorCode};

/// `displayName` 的长度上界（§5.2/§5.5 的 `≤128`）。
const MAX_DISPLAY_NAME_CHARS: usize = 128;

/// `device.pair.reject`/`node.pair.reject` 的 `reason` 长度上界：与 `core::model` 的
/// `PairingSettlement::rejected` 是同一边界（§5.3/§5.4 只登记 `reason: string | null`，长度由核心
/// 值对象决定）。本层提前判，使超长回 `local.invalid_params` 而不是端口错误。
const MAX_REJECT_REASON_CHARS: usize = 256;

/// `local.invalid_params`（§6）。
pub fn invalid_params(message: impl AsRef<str>) -> AdminError {
    AdminError::new(LocalErrorCode::InvalidParams, message)
}

// ---------------------------------------------------------------------------------------------
// 通用取值助手（closed object + 类型校验）
// ---------------------------------------------------------------------------------------------

/// 拒绝本文档未登记的 `params` 字段（§1.1：`params` 是 closed object）。
pub(crate) fn reject_unknown_fields(
    params: &JsonObject,
    allowed: &[&str],
) -> Result<(), AdminError> {
    match params.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(invalid_params(format!("unknown parameter `{key}`"))),
        None => Ok(()),
    }
}

/// 取一个必需的字段（缺失即 `local.invalid_params`）。
fn value<'a>(params: &'a JsonObject, key: &str) -> Result<&'a Value, AdminError> {
    params
        .get(key)
        .ok_or_else(|| invalid_params(format!("missing parameter `{key}`")))
}

fn string(params: &JsonObject, key: &str) -> Result<String, AdminError> {
    value(params, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid_params(format!("parameter `{key}` must be a string")))
}

/// `null` 与非 `null` 两态的可选字符串（§5.2 的 `graceMs`/§5.6 的 `since`/`until` 风格）。
fn optional_string(params: &JsonObject, key: &str) -> Result<Option<String>, AdminError> {
    match value(params, key)? {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text.clone())),
        _ => Err(invalid_params(format!(
            "parameter `{key}` must be a string or null"
        ))),
    }
}

fn object<'a>(params: &'a JsonObject, key: &str) -> Result<&'a JsonObject, AdminError> {
    match value(params, key)? {
        Value::Object(map) => Ok(map),
        _ => Err(invalid_params(format!(
            "parameter `{key}` must be an object"
        ))),
    }
}

fn array<'a>(params: &'a JsonObject, key: &str) -> Result<&'a Vec<Value>, AdminError> {
    match value(params, key)? {
        Value::Array(items) => Ok(items),
        _ => Err(invalid_params(format!(
            "parameter `{key}` must be an array"
        ))),
    }
}

fn strings(params: &JsonObject, key: &str) -> Result<Vec<String>, AdminError> {
    array(params, key)?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid_params(format!("every item of `{key}` must be a string")))
        })
        .collect()
}

/// 布尔字段。
fn boolean(params: &JsonObject, key: &str) -> Result<bool, AdminError> {
    value(params, key)?
        .as_bool()
        .ok_or_else(|| invalid_params(format!("parameter `{key}` must be a boolean")))
}

/// `displayName` 形态：1..=128 个字符（Unicode code point，与 JSON Schema 的 `maxLength` 同口径）。
fn display_name(params: &JsonObject, key: &str) -> Result<String, AdminError> {
    let text = string(params, key)?;
    let count = text.chars().count();
    if count == 0 || count > MAX_DISPLAY_NAME_CHARS {
        return Err(invalid_params(format!(
            "parameter `{key}` must be 1..={MAX_DISPLAY_NAME_CHARS} characters"
        )));
    }
    Ok(text)
}

/// `^[A-Za-z0-9._-]{1,max}$`（`providerId` 与 `values` 的字段名；core 未导出这两个谓词）。
fn is_spec_name(text: &str, max: usize) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= max
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn spec_name(params: &JsonObject, key: &str, max: usize) -> Result<String, AdminError> {
    let text = string(params, key)?;
    if !is_spec_name(&text, max) {
        return Err(invalid_params(format!(
            "parameter `{key}` must match ^[A-Za-z0-9._-]{{1,{max}}}$"
        )));
    }
    Ok(text)
}

/// `^[A-Za-z0-9._-]{1,max}$` 形态判定（供嵌套字段复用）。
fn require_spec_name(text: &str, max: usize, what: &str) -> Result<(), AdminError> {
    if is_spec_name(text, max) {
        Ok(())
    } else {
        Err(invalid_params(format!(
            "{what} must match ^[A-Za-z0-9._-]{{1,{max}}}$"
        )))
    }
}

/// §1.1 的时间戳文本（UTC、毫秒、`Z`）。
fn timestamp(params: &JsonObject, key: &str) -> Result<Option<Timestamp>, AdminError> {
    match optional_string(params, key)? {
        Some(text) => Timestamp::new(&text).map(Some).map_err(|_| {
            invalid_params(format!(
                "parameter `{key}` must be a UTC RFC 3339 timestamp"
            ))
        }),
        None => Ok(None),
    }
}

/// 由 core 的具名构造错误生成 `local.invalid_params`（词表在 core，本层只翻译错误码）。
fn from_invalid(what: &str, error: acp_core::model::InvalidValue) -> AdminError {
    invalid_params(format!("{what} is invalid: {error}"))
}

// ---------------------------------------------------------------------------------------------
// daemon.*（§5.2）
// ---------------------------------------------------------------------------------------------

/// `daemon.status`：无参数（§1.1：无参数方法必须发送 `{}`）。
pub fn daemon_status(params: &JsonObject) -> Result<(), AdminError> {
    reject_unknown_fields(params, &[])
}

/// `daemon.stop` 的 `graceMs`：`null` = 使用配置值，非空时 `0..=60000`（§5.2）。
pub fn daemon_stop(params: &JsonObject) -> Result<Option<u64>, AdminError> {
    reject_unknown_fields(params, &["graceMs"])?;
    match value(params, "graceMs")? {
        Value::Null => Ok(None),
        Value::Number(number) => {
            let grace_ms = number.as_u64().ok_or_else(|| {
                invalid_params("parameter `graceMs` must be an integer in 0..=60000")
            })?;
            if grace_ms > MAX_STOP_GRACE_MS {
                return Err(invalid_params(format!(
                    "parameter `graceMs` must be at most {MAX_STOP_GRACE_MS}"
                )));
            }
            Ok(Some(grace_ms))
        }
        _ => Err(invalid_params(
            "parameter `graceMs` must be an integer or null",
        )),
    }
}

// ---------------------------------------------------------------------------------------------
// workspace.select / agent.configure / provider.configure（§5.2）
// ---------------------------------------------------------------------------------------------

/// `workspace.select` 的已校验参数：`root_path` 是 **canonicalize 后**的持久权威值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSelect {
    /// 符号名（发布给 Export 的名字）。
    pub alias: WorkspaceAlias,
    /// 展示名。
    pub display_name: String,
    /// 规范化后的绝对目录路径。
    pub canonical_path: String,
}

/// `workspace.select`（§5.2）：`alias`/`displayName`/`rootPath`。
pub fn workspace_select(params: &JsonObject) -> Result<WorkspaceSelect, AdminError> {
    reject_unknown_fields(params, &["alias", "displayName", "rootPath"])?;
    let alias_text = string(params, "alias")?;
    let alias = WorkspaceAlias::new(&alias_text)
        .map_err(|error| from_invalid("parameter `alias`", error))?;
    let display_name = display_name(params, "displayName")?;
    let root_path = string(params, "rootPath")?;
    let canonical_path = canonical_workspace_path(&root_path)?;
    Ok(WorkspaceSelect {
        alias,
        display_name,
        canonical_path,
    })
}

/// §5.2 的路径规则：绝对、不含 `..`、已存在且是目录；写入前 `canonicalize`，持久值取规范化结果。
///
/// 校验顺序固定（绝对 → 无 `..` → 存在 → 是目录），因此拒绝原因可判定、不依赖平台错误文本。
fn canonical_workspace_path(text: &str) -> Result<String, AdminError> {
    let path = Path::new(text);
    if text.is_empty() || !path.is_absolute() {
        return Err(invalid_params(
            "parameter `rootPath` must be an absolute path",
        ));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(invalid_params("parameter `rootPath` must not contain `..`"));
    }
    let metadata = std::fs::metadata(path)
        .map_err(|_| invalid_params("parameter `rootPath` must be an existing directory"))?;
    if !metadata.is_dir() {
        return Err(invalid_params("parameter `rootPath` must be a directory"));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| invalid_params("parameter `rootPath` cannot be canonicalized"))?;
    // 规范路径必须是可用 UTF-8：core 的 `WorkspaceRecord` 与 wire 都是文本。
    canonical
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid_params("parameter `rootPath` must be a UTF-8 path"))
}

/// `agent.configure` 的已校验参数（§5.2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConfigure {
    /// profile 标识（1..=128 字符，跨节点传输时不得比 wire 更严）。
    pub agent_id: AgentId,
    /// 展示名。
    pub display_name: String,
    /// 可执行文件路径或名字（不经 shell 拼接）。
    pub command: String,
    /// 逐项传给子进程的参数（不拼接）。
    pub args: Vec<String>,
    /// 环境变量名白名单（注入上限）。
    pub env_allowlist: Vec<String>,
    /// 是否设为默认 profile。
    pub default: bool,
}

/// `agent.configure`（§5.2）：六个字段都是必需字段。
pub fn agent_configure(params: &JsonObject) -> Result<AgentConfigure, AdminError> {
    reject_unknown_fields(
        params,
        &[
            "agentId",
            "displayName",
            "command",
            "args",
            "envAllowlist",
            "default",
        ],
    )?;
    let agent_id_text = string(params, "agentId")?;
    let agent_id =
        AgentId::new(&agent_id_text).map_err(|error| from_invalid("parameter `agentId`", error))?;
    Ok(AgentConfigure {
        agent_id,
        display_name: display_name(params, "displayName")?,
        command: string(params, "command")?,
        args: strings(params, "args")?,
        env_allowlist: strings(params, "envAllowlist")?,
        default: boolean(params, "default")?,
    })
}

/// `provider.configure` 的已校验参数（§5.2）：`values` **只承载凭据值**，保持客户端给出的顺序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfigure {
    /// Provider 标识。
    pub provider_id: String,
    /// 类别。
    pub kind: ProviderRefKind,
    /// 展示名。
    pub display_name: String,
    /// 字段名 → 凭据文本（至少 1 项）。
    pub values: Vec<(String, String)>,
}

/// `provider.configure`（§5.2）：`values` 至少 1 项，字段名 `^[A-Za-z0-9._-]{1,64}$`，值非空字符串。
///
/// 值非空的理由是可判定性：keystore 的 Provider 凭据条目要求长度 ≥1，空值只会在端口层报错；
/// 这里提前判定为参数非法，避免把「参数写错」报成「keystore 不可用」。
pub fn provider_configure(params: &JsonObject) -> Result<ProviderConfigure, AdminError> {
    reject_unknown_fields(params, &["providerId", "kind", "displayName", "values"])?;
    let provider_id = spec_name(params, "providerId", 64)?;
    let kind_text = string(params, "kind")?;
    let kind = kind_text
        .parse::<ProviderRefKind>()
        .map_err(|_| invalid_params("parameter `kind` must be `provider` or `mcp`"))?;
    let display_name = display_name(params, "displayName")?;
    let values = object(params, "values")?;
    if values.is_empty() {
        return Err(invalid_params(
            "parameter `values` must contain at least one credential field",
        ));
    }
    let mut fields = Vec::with_capacity(values.len());
    for (field, raw) in values {
        require_spec_name(field, 64, &format!("parameter `values` field `{field}`"))?;
        let secret = raw.as_str().ok_or_else(|| {
            invalid_params(format!("parameter `values.{field}` must be a string"))
        })?;
        if secret.is_empty() {
            return Err(invalid_params(format!(
                "parameter `values.{field}` must not be empty"
            )));
        }
        fields.push((field.clone(), secret.to_owned()));
    }
    Ok(ProviderConfigure {
        provider_id,
        kind,
        display_name,
        values: fields,
    })
}

// ---------------------------------------------------------------------------------------------
// export.* / import.*（§5.5）
// ---------------------------------------------------------------------------------------------

/// `export.create`（§5.5）：`ExportView` 去掉 `createdAt`/`revokedAt` 后的全部字段，均为必需。
///
/// 首切片约束在此判定：`agentIds`/`workspaceAliases`/`templates` 恰好 1 项、`templates[].params`
/// 必须为空、`cachePolicy` 固定 `no-content-cache`；`default*` 一致性由 `ExportRecord` 的不变式
/// 兜底（core 只有一份定义）。
pub fn export_create(
    params: &JsonObject,
    created_at: Timestamp,
) -> Result<ExportRecord, AdminError> {
    reject_unknown_fields(
        params,
        &[
            "exportId",
            "displayName",
            "agentIds",
            "workspaceAliases",
            "defaultWorkspaceAlias",
            "templates",
            "defaultTemplateId",
            "scopes",
            "cachePolicy",
        ],
    )?;

    let export_id_text = string(params, "exportId")?;
    let export_id = ExportId::new(&export_id_text)
        .map_err(|error| from_invalid("parameter `exportId`", error))?;
    let display = display_name(params, "displayName")?;

    let agent_ids_raw = strings(params, "agentIds")?;
    if agent_ids_raw.len() != 1 {
        return Err(invalid_params(
            "parameter `agentIds` must contain exactly one agent in this slice",
        ));
    }
    let agent_ids = agent_ids_raw
        .iter()
        .map(|text| AgentId::new(text).map_err(|error| from_invalid("parameter `agentIds`", error)))
        .collect::<Result<Vec<_>, _>>()?;

    let aliases_raw = array(params, "workspaceAliases")?;
    if aliases_raw.len() != 1 {
        return Err(invalid_params(
            "parameter `workspaceAliases` must contain exactly one entry in this slice",
        ));
    }
    let mut workspace_aliases = Vec::with_capacity(aliases_raw.len());
    for (index, entry) in aliases_raw.iter().enumerate() {
        let entry = entry.as_object().ok_or_else(|| {
            invalid_params(format!("`workspaceAliases[{index}]` must be an object"))
        })?;
        reject_unknown_fields(entry, &["alias", "displayName"])?;
        let alias_text = string(entry, "alias")?;
        let alias = WorkspaceAlias::new(&alias_text)
            .map_err(|error| from_invalid(&format!("`workspaceAliases[{index}].alias`"), error))?;
        workspace_aliases.push(
            WorkspaceAliasEntry::try_new(alias, &display_name(entry, "displayName")?)
                .map_err(|error| from_invalid("`workspaceAliases`", error))?,
        );
    }

    let default_alias_text = string(params, "defaultWorkspaceAlias")?;
    let default_workspace_alias = WorkspaceAlias::new(&default_alias_text)
        .map_err(|error| from_invalid("parameter `defaultWorkspaceAlias`", error))?;

    let templates_raw = array(params, "templates")?;
    if templates_raw.len() != 1 {
        return Err(invalid_params(
            "parameter `templates` must contain exactly one template in this slice",
        ));
    }
    let mut templates = Vec::with_capacity(templates_raw.len());
    for (index, entry) in templates_raw.iter().enumerate() {
        let entry = entry
            .as_object()
            .ok_or_else(|| invalid_params(format!("`templates[{index}]` must be an object")))?;
        reject_unknown_fields(
            entry,
            &["templateId", "displayName", "workspaceAlias", "params"],
        )?;
        let template_id_text = string(entry, "templateId")?;
        let template_id = TemplateId::new(&template_id_text)
            .map_err(|error| from_invalid(&format!("`templates[{index}].templateId`"), error))?;
        let workspace_alias_text = string(entry, "workspaceAlias")?;
        let workspace_alias = WorkspaceAlias::new(&workspace_alias_text).map_err(|error| {
            from_invalid(&format!("`templates[{index}].workspaceAlias`"), error)
        })?;
        // §5.5：首切片的 template 不得声明参数（有参 template 属 `post_mvp`，不能静默忽略）。
        if !array(entry, "params")?.is_empty() {
            return Err(invalid_params(format!(
                "`templates[{index}].params` must be empty in this slice"
            )));
        }
        templates.push(
            ExportTemplate::try_new(
                template_id,
                &display_name(entry, "displayName")?,
                workspace_alias,
                Vec::new(),
            )
            .map_err(|error| from_invalid(&format!("`templates[{index}]`"), error))?,
        );
    }

    let default_template_text = string(params, "defaultTemplateId")?;
    let default_template_id = TemplateId::new(&default_template_text)
        .map_err(|error| from_invalid("parameter `defaultTemplateId`", error))?;

    let scopes = GrantSet::try_from_iter(strings(params, "scopes")?)
        .map_err(|error| from_invalid("parameter `scopes`", error))?;

    let cache_policy_text = string(params, "cachePolicy")?;
    if cache_policy_text != CachePolicy::NoContentCache.as_str() {
        return Err(invalid_params(
            "parameter `cachePolicy` must be `no-content-cache` in v1",
        ));
    }

    ExportRecord::try_new(
        export_id,
        &display,
        agent_ids,
        workspace_aliases,
        default_workspace_alias,
        templates,
        default_template_id,
        scopes,
        CachePolicy::NoContentCache,
        created_at,
        None,
    )
    .map_err(|error| from_invalid("export request", error))
}

/// `export.revoke`（§5.5）：`{ exportId }`。
pub fn export_revoke(params: &JsonObject) -> Result<ExportId, AdminError> {
    reject_unknown_fields(params, &["exportId"])?;
    let text = string(params, "exportId")?;
    ExportId::new(&text).map_err(|error| from_invalid("parameter `exportId`", error))
}

/// `import.remove`（§5.5）：`{ importId }`。
pub fn import_remove(params: &JsonObject) -> Result<ImportId, AdminError> {
    reject_unknown_fields(params, &["importId"])?;
    let text = string(params, "importId")?;
    ImportId::new(&text).map_err(|error| from_invalid("parameter `importId`", error))
}

// ---------------------------------------------------------------------------------------------
// audit.export（§5.6）
// ---------------------------------------------------------------------------------------------

/// 审计导出的文件格式（§5.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditFormat {
    /// `jsonl`：每行一个 JSON 对象。
    Jsonl,
    /// `csv`：首行为列名。
    Csv,
}

impl AuditFormat {
    /// 解析 wire 文本（§5.6：`jsonl` | `csv`）。
    fn parse(text: &str) -> Option<Self> {
        match text {
            "jsonl" => Some(Self::Jsonl),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}

/// `audit.export` 的已校验参数（§5.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditExport {
    /// 输出文件路径（绝对路径；文件已存在属 `local.conflict`，由写出阶段判定）。
    pub output_path: PathBuf,
    /// 输出格式。
    pub format: AuditFormat,
    /// 时间区间下界（含）。
    pub since: Option<Timestamp>,
    /// 时间区间上界（含）。
    pub until: Option<Timestamp>,
    /// §14.2 的审计类别名；空数组 = 不按类别过滤（必须显式给出该字段）。
    pub categories: Vec<AuditAction>,
}

/// `audit.export`（§5.6）：`outputPath` 必须绝对；`categories` 必须显式给出（可为空数组）。
pub fn audit_export(params: &JsonObject) -> Result<AuditExport, AdminError> {
    reject_unknown_fields(
        params,
        &["outputPath", "format", "since", "until", "categories"],
    )?;

    let output_path = string(params, "outputPath")?;
    if output_path.is_empty() || !Path::new(&output_path).is_absolute() {
        return Err(invalid_params(
            "parameter `outputPath` must be an absolute path",
        ));
    }

    let format_text = string(params, "format")?;
    let format = AuditFormat::parse(&format_text)
        .ok_or_else(|| invalid_params("parameter `format` must be `jsonl` or `csv`"))?;

    let since = timestamp(params, "since")?;
    let until = timestamp(params, "until")?;

    let mut categories = Vec::new();
    for category in strings(params, "categories")? {
        let action = category.parse::<AuditAction>().map_err(|_| {
            invalid_params(format!(
                "parameter `categories` contains `{category}`, which is not a documented audit category"
            ))
        })?;
        if !categories.contains(&action) {
            categories.push(action);
        }
    }

    Ok(AuditExport {
        output_path: PathBuf::from(output_path),
        format,
        since,
        until,
        categories,
    })
}

// ---------------------------------------------------------------------------------------------
// device.pair.* / device.list / device.revoke / node.pair.* / node.list / node.revoke（§5.3/§5.4）
// ---------------------------------------------------------------------------------------------

/// `device.pair.begin` 的已校验参数（§5.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePairBegin {
    /// 展开后的独立 scope 集合（`pack.*` 已按 `SECURITY_DESIGN.md` §10.2 展开；不含包名）。
    pub scopes: ScopeSet,
    /// 请求的有效期（毫秒）；`None` = 用满 5 分钟窗口，只能收窄。
    pub expires_in_ms: Option<u64>,
}

/// `device.pair.begin`（§5.3）：`requestedPacks`/`requestedScopes` 可为空，`expiresInMs` 可空。
///
/// 取值域**只**允许 `pack.*` 与命令名（§5.3 的原文）；`preset.*` 不在本方法的输入域内——CLI 若要提供
/// 预设，应先在本机把它展开成 packs，而不是把它当作 pack 名传进来（展开只经
/// `identity_auth::authorization` 这一份词表）。
pub fn device_pair_begin(params: &JsonObject) -> Result<DevicePairBegin, AdminError> {
    reject_unknown_fields(
        params,
        &["requestedPacks", "requestedScopes", "expiresInMs"],
    )?;
    let packs = strings(params, "requestedPacks")?;
    let requested = strings(params, "requestedScopes")?;
    let scopes = authorization::expand_device_request(&packs, &requested).map_err(|_| {
        invalid_params(
            "device.pair.begin: `requestedPacks`/`requestedScopes` must name known packs or command scopes",
        )
    })?;
    Ok(DevicePairBegin {
        scopes,
        expires_in_ms: pairing_window(params)?,
    })
}

/// `node.pair.begin` 的已校验参数（§5.4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodePairBegin {
    /// `mode = "owner"`：本节点创建一次性配对并返回二维码 URL。
    Owner {
        /// 本节点向对端宣告的展示名（`nodeName`）。
        display_name: String,
        /// 本机允许授予的 `grant.*` 上限（`node.pair.confirm` 的最终集合必须落在它之内）。
        grants: GrantSet,
    },
    /// `mode = "access"`：本切片不支持（路由层回 `local.unsupported`，不发任何出站请求）。
    Access,
}

/// `node.pair.begin`（§5.4）：`mode` 决定分支；`owner` 分支要求 `pairingUrl` 为 `null`。
///
/// `mode = "access"` 时**不继续解析**其余字段：本切片不实现该模式，把「不支持」报成「参数非法」
/// 会误导调用方（§5.4 的 `mode = "access"` 分支与 `node.rotate-key.begin` 同类）。
///
/// `requestedGrants` 在两个模式下都是**本机登记的授权上限**：`owner` 模式下对端尚未认领，本机必须先
/// 登记一个上限，`node.pair.confirm` 的最终 grants 才能通过「不得超出请求集合」的校验（§5.4 的确认
/// 场景要求确认后记录带初始 `grant.*`）。
pub fn node_pair_begin(params: &JsonObject) -> Result<NodePairBegin, AdminError> {
    reject_unknown_fields(
        params,
        &["mode", "pairingUrl", "displayName", "requestedGrants"],
    )?;
    let mode = string(params, "mode")?;
    match mode.as_str() {
        "access" => Ok(NodePairBegin::Access),
        "owner" => {
            if optional_string(params, "pairingUrl")?.is_some() {
                return Err(invalid_params(
                    "node.pair.begin: `pairingUrl` must be null when `mode` is `owner`",
                ));
            }
            let display_name = display_name(params, "displayName")?;
            let requested = GrantSet::try_from_iter(strings(params, "requestedGrants")?)
                .map_err(|error| from_invalid("parameter `requestedGrants`", error))?;
            let expanded = authorization::expand_node_request(
                &requested.iter().map(str::to_owned).collect::<Vec<_>>(),
            )
            .map_err(|_| {
                invalid_params("node.pair.begin: `requestedGrants` must name known grants")
            })?;
            Ok(NodePairBegin::Owner {
                display_name,
                grants: expanded.grants,
            })
        }
        _ => Err(invalid_params(
            "parameter `mode` must be `owner` or `access`",
        )),
    }
}

/// `device.pair.confirm` 的已校验参数（§5.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePairConfirm {
    /// 配对目标。
    pub pairing_id: PairingId,
    /// 用户确认的最终 scope 集合（不是请求值；必须完整展示后确认）。
    pub scopes: ScopeSet,
}

/// `device.pair.confirm`（§5.3）：`{ pairingId, scopes }`；设备配对不带 grants（未知字段即拒绝）。
pub fn device_pair_confirm(params: &JsonObject) -> Result<DevicePairConfirm, AdminError> {
    reject_unknown_fields(params, &["pairingId", "scopes"])?;
    let pairing_id = pairing_id_field(params)?;
    let scopes = ScopeSet::try_from_iter(strings(params, "scopes")?)
        .map_err(|error| from_invalid("parameter `scopes`", error))?;
    Ok(DevicePairConfirm { pairing_id, scopes })
}

/// `node.pair.confirm` 的已校验参数（§5.4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePairConfirm {
    /// 配对目标。
    pub pairing_id: PairingId,
    /// 用户确认的最终 `grant.*` 集合（不是请求值）。
    pub grants: GrantSet,
}

/// `node.pair.confirm`（§5.4）：`{ pairingId, grants }`；节点配对不带 scopes（未知字段即拒绝）。
pub fn node_pair_confirm(params: &JsonObject) -> Result<NodePairConfirm, AdminError> {
    reject_unknown_fields(params, &["pairingId", "grants"])?;
    let pairing_id = pairing_id_field(params)?;
    let grants = GrantSet::try_from_iter(strings(params, "grants")?)
        .map_err(|error| from_invalid("parameter `grants`", error))?;
    Ok(NodePairConfirm { pairing_id, grants })
}

/// `device.pair.reject` / `node.pair.reject` 的已校验参数（§5.3/§5.4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingReject {
    /// 配对目标。
    pub pairing_id: PairingId,
    /// 可选简短原因（≤150 字符；不进审计正文）。
    pub reason: Option<String>,
}

/// `device.pair.reject` / `node.pair.reject`（§5.3/§5.4）：`{ pairingId, reason }`。
pub fn pairing_reject(params: &JsonObject) -> Result<PairingReject, AdminError> {
    reject_unknown_fields(params, &["pairingId", "reason"])?;
    let pairing_id = pairing_id_field(params)?;
    let reason = optional_string(params, "reason")?;
    if let Some(reason) = &reason {
        if reason.chars().count() > MAX_REJECT_REASON_CHARS {
            return Err(invalid_params(format!(
                "parameter `reason` must be at most {MAX_REJECT_REASON_CHARS} characters"
            )));
        }
    }
    Ok(PairingReject { pairing_id, reason })
}

/// `device.pair.status` / `device.pair.reject` / `node.pair.status` / `node.pair.reject` 的 `pairingId`。
pub fn pairing_id(params: &JsonObject) -> Result<PairingId, AdminError> {
    reject_unknown_fields(params, &["pairingId"])?;
    pairing_id_field(params)
}

/// `device.revoke`（§5.3）：`{ deviceId }`。
pub fn device_revoke(params: &JsonObject) -> Result<DeviceId, AdminError> {
    reject_unknown_fields(params, &["deviceId"])?;
    let text = string(params, "deviceId")?;
    DeviceId::new(&text).map_err(|error| from_invalid("parameter `deviceId`", error))
}

/// `node.revoke`（§5.4）：`{ nodeId }`。
pub fn node_revoke(params: &JsonObject) -> Result<NodeId, AdminError> {
    reject_unknown_fields(params, &["nodeId"])?;
    let text = string(params, "nodeId")?;
    NodeId::new(&text).map_err(|error| from_invalid("parameter `nodeId`", error))
}

/// 已确认/已拒绝的判定（§5.3/§5.4、spec 「过期与拒绝不创建信任」）：
///
/// - 记录已过期（含时钟判定的过期）或已被拒绝 → `local.expired`（§6：`local.expired` 专服务这一对
///   语义，"`pairingId` 已过期或已被拒绝"）；
/// - 仍未认领，或已批准/已消费 → `local.conflict`（状态不允许该操作）；
/// - 否则放行（`pending_confirmation` 且未过期）。
///
/// 这里**不**替代状态机与存储的判定：放行后仍由 `Authority::settle` 与写集做同一组检查，本层只负责把
/// 「终态」映射成协议文档规定的错误码（存储对 `rejected` 回的是 `ConflictKind::Consumed`，直接透传会
/// 变成 `local.conflict`，与 spec 要求的 `local.expired` 不符）。
pub fn require_pending_confirmation(
    operation: &str,
    record: &PairingRecord,
    now: &Timestamp,
) -> Result<(), AdminError> {
    // 时钟判定的过期与记录状态无关：从未认领的 `created` 配对超过 `expiresAt` 后同样是「已过期」（存储
    // 的过期扫描滞后于调用，路由不能等它才给出正确错误码）。
    let expired_by_clock = now.as_str() >= record.expires_at().as_str();
    match record.state() {
        PairingState::Rejected | PairingState::Expired => Err(expired(operation)),
        PairingState::Created | PairingState::PendingConfirmation if expired_by_clock => {
            Err(expired(operation))
        }
        PairingState::PendingConfirmation => Ok(()),
        _ => Err(AdminError::new(
            LocalErrorCode::Conflict,
            format!(
                "{operation}: pairing state `{}` does not allow this operation",
                record.state().as_str()
            ),
        )),
    }
}

/// `local.expired`（§6：`pairingId` 已过期或已被拒绝）。
fn expired(operation: &str) -> AdminError {
    AdminError::new(
        LocalErrorCode::Expired,
        format!("{operation}: pairing has expired or was rejected"),
    )
}

/// 用户确认的最终集合不得超出请求集合（§5.3/§5.4）：越界是**参数非法**（不是状态冲突），
/// 与 `local.invalid_params` 的「越界」定义一致（§6）。
pub fn require_confirm_subset<'a>(
    operation: &str,
    field: &str,
    granted: impl Iterator<Item = &'a str>,
    requested: impl Iterator<Item = &'a str>,
) -> Result<(), AdminError> {
    let requested: Vec<&str> = requested.collect();
    if granted.into_iter().all(|name| requested.contains(&name)) {
        Ok(())
    } else {
        Err(invalid_params(format!(
            "{operation}: `{field}` must not exceed the requested set"
        )))
    }
}

/// `expiresInMs`（§5.3）：`null` = 用满 5 分钟；非空时 `1..=PAIRING_WINDOW_MS`（只能收窄）。
fn pairing_window(params: &JsonObject) -> Result<Option<u64>, AdminError> {
    match value(params, "expiresInMs")? {
        Value::Null => Ok(None),
        Value::Number(number) => {
            let window = number.as_u64().filter(|window| {
                *window > 0 && *window <= crate::local_admin::pairing::PAIRING_WINDOW_MS
            });
            window.map(Some).ok_or_else(|| {
                invalid_params(format!(
                    "parameter `expiresInMs` must be an integer in 1..={}",
                    crate::local_admin::pairing::PAIRING_WINDOW_MS
                ))
            })
        }
        _ => Err(invalid_params(
            "parameter `expiresInMs` must be an integer or null",
        )),
    }
}

/// `pairingId`：canonical 小写 UUID（core 的 [`PairingId`] 是唯一校验入口）。
fn pairing_id_field(params: &JsonObject) -> Result<PairingId, AdminError> {
    let text = string(params, "pairingId")?;
    PairingId::new(&text).map_err(|error| from_invalid("parameter `pairingId`", error))
}

// ---------------------------------------------------------------------------------------------
// PortError → local.*（§6）
// ---------------------------------------------------------------------------------------------

/// `PortError` → `local.*`（§6；映射表逐条见报告与 `LOCAL_ADMIN_PROTOCOL.md` §6）。
///
/// 口径：
///
/// - `NotFound` → `local.not_found`；`Conflict` → `local.conflict`，但 `ConflictKind::Expired`
///   （`pairing.expired`）按 §6 映射为 `local.expired`；
/// - `Unavailable(_)`（含 `KeystoreUnavailable`）→ `local.unavailable`；
/// - `InvalidRequest`（含 `require_local` 的 `authorization.scope_denied`）→ `local.invalid_params`；
/// - `Corrupt`/`Backend` → `local.internal`，且**不转述**内层文本（适配器文本可能含 SQL、路径或 prompt）。
///
/// `operation` 只允许传方法名常量：错误消息与日志里不得出现对端给的自由文本值。
pub fn map_port_error(operation: &str, error: PortError) -> AdminError {
    match error {
        PortError::NotFound(target) => AdminError::new(
            LocalErrorCode::NotFound,
            format!("{operation}: {target} does not exist"),
        ),
        PortError::Conflict(ConflictKind::Expired) => AdminError::new(
            LocalErrorCode::Expired,
            format!("{operation}: pairing has expired or was rejected"),
        ),
        PortError::Conflict(kind) => AdminError::new(
            LocalErrorCode::Conflict,
            format!("{operation}: {kind} conflicts with the current state"),
        ),
        PortError::Unavailable(kind) => {
            AdminError::new(LocalErrorCode::Unavailable, format!("{operation}: {kind}"))
        }
        PortError::InvalidRequest(reason) => AdminError::new(
            LocalErrorCode::InvalidParams,
            format!("{operation}: {reason}"),
        ),
        PortError::Corrupt(_) | PortError::Backend(_) => AdminError::new(
            LocalErrorCode::Internal,
            format!("{operation}: unexpected internal error"),
        ),
    }
}

/// `identity_auth::PairingError` → `local.*`（§5.3/§5.4、§6）。
///
/// 映射口径（与 [`map_port_error`] 同源）：
///
/// - `Expired` → `local.expired`；
/// - `WrongState`/`NotClaimable` → `local.conflict`（状态不允许该操作）；
/// - 集合越界/窗口非法/目标族不匹配 → `local.invalid_params`（参数越界）；
/// - `SecretUnavailable`/`EntropyUnavailable` → `local.unavailable`（重启或熵源不可用时重试语义）；
/// - 其余（transcript/证明/绑定/标识不一致）→ `local.internal`，且**不转述**内层文本：
///   `error.message` 必须是简短英文描述，内层的诊断是中文且可能回显对端输入（§4 规则 6）。
pub fn map_pairing_error(operation: &str, error: PairingError) -> AdminError {
    match error {
        PairingError::Expired => AdminError::new(
            LocalErrorCode::Expired,
            format!("{operation}: pairing has expired or was rejected"),
        ),
        PairingError::WrongState | PairingError::NotClaimable => AdminError::new(
            LocalErrorCode::Conflict,
            format!("{operation}: pairing state does not allow this operation"),
        ),
        PairingError::CapabilityKindMismatch
        | PairingError::InvalidWindow
        | PairingError::TargetMismatch
        | PairingError::CapabilitiesExceedRegistered
        | PairingError::CapabilitiesExceedRequested
        | PairingError::Invalid(_) => invalid_params(format!(
            "{operation}: pairing parameters or sets are out of the allowed range"
        )),
        PairingError::SecretUnavailable => AdminError::new(
            LocalErrorCode::Unavailable,
            format!("{operation}: pairing secret is no longer held in memory"),
        ),
        PairingError::EntropyUnavailable => AdminError::new(
            LocalErrorCode::Unavailable,
            format!("{operation}: system entropy is not available"),
        ),
        PairingError::ClaimMismatch
        | PairingError::BindingMismatch
        | PairingError::Proof(_)
        | PairingError::Transcript(_) => AdminError::new(
            LocalErrorCode::Internal,
            format!("{operation}: pairing state machine rejected the stored facts"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::json;

    use super::*;

    const TS: &str = "2026-09-18T09:12:03.412Z";

    fn params(value: serde_json::Value) -> JsonObject {
        match value {
            Value::Object(map) => map,
            other => panic!("测试参数必须是 object：{other}"),
        }
    }

    fn temp_directory() -> PathBuf {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "acpr-wp3b1-params-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("临时目录可建");
        path
    }

    fn code_of(error: AdminError) -> LocalErrorCode {
        error.code()
    }

    // ---------------------------------------------------------------- daemon.*

    #[test]
    fn daemon_status_rejects_any_parameter() {
        assert!(daemon_status(&JsonObject::new()).is_ok());
        assert_eq!(
            code_of(daemon_status(&params(json!({"graceMs": 1}))).expect_err("未知字段")),
            LocalErrorCode::InvalidParams
        );
    }

    #[test]
    fn daemon_stop_checks_the_grace_bounds() {
        assert_eq!(
            daemon_stop(&params(json!({"graceMs": null}))).unwrap(),
            None
        );
        assert_eq!(
            daemon_stop(&params(json!({"graceMs": 0}))).unwrap(),
            Some(0)
        );
        assert_eq!(
            daemon_stop(&params(json!({"graceMs": 60_000}))).unwrap(),
            Some(60_000)
        );
        for rejected in [
            json!({"graceMs": 60_001}),
            json!({"graceMs": -1}),
            json!({"graceMs": 1.5}),
            json!({"graceMs": "100"}),
            json!({}),
        ] {
            assert_eq!(
                code_of(daemon_stop(&params(rejected.clone())).expect_err(&rejected.to_string())),
                LocalErrorCode::InvalidParams,
                "{rejected}"
            );
        }
    }

    // ---------------------------------------------------------------- workspace.select

    #[test]
    fn workspace_select_canonicalizes_an_existing_directory() {
        let directory = temp_directory();
        let select = workspace_select(&params(json!({
            "alias": "project.1",
            "displayName": "Project One",
            "rootPath": directory.to_str().expect("utf-8"),
        })))
        .expect("合法目录必须通过");
        assert_eq!(select.alias.as_str(), "project.1");
        assert_eq!(select.display_name, "Project One");
        let expected = fs::canonicalize(&directory).expect("可规范化");
        assert_eq!(select.canonical_path, expected.to_str().expect("utf-8"));
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn workspace_select_rejects_relative_parented_missing_and_file_paths() {
        let directory = temp_directory();
        let file = directory.join("plain.txt");
        fs::write(&file, b"x").expect("可写");

        let relative = workspace_select(&params(json!({
            "alias": "a", "displayName": "A", "rootPath": "relative/path",
        })))
        .expect_err("相对路径必须被拒");
        assert_eq!(code_of(relative), LocalErrorCode::InvalidParams);

        let parented = workspace_select(&params(json!({
            "alias": "a",
            "displayName": "A",
            "rootPath": format!("{}/../{}", directory.to_str().expect("utf-8"), "sub"),
        })))
        .expect_err("含 `..` 必须被拒");
        assert_eq!(code_of(parented), LocalErrorCode::InvalidParams);

        let missing = workspace_select(&params(json!({
            "alias": "a",
            "displayName": "A",
            "rootPath": directory.join("missing").to_str().expect("utf-8"),
        })))
        .expect_err("不存在的目录必须被拒");
        assert_eq!(code_of(missing), LocalErrorCode::InvalidParams);

        let not_a_directory = workspace_select(&params(json!({
            "alias": "a",
            "displayName": "A",
            "rootPath": file.to_str().expect("utf-8"),
        })))
        .expect_err("文件不是目录");
        assert_eq!(code_of(not_a_directory), LocalErrorCode::InvalidParams);

        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn workspace_select_checks_alias_and_display_name_shapes() {
        let directory = temp_directory();
        let root = directory.to_str().expect("utf-8").to_owned();
        for alias in ["Project", "", "alias/with/slash", "-alias"] {
            assert_eq!(
                code_of(
                    workspace_select(&params(json!({
                        "alias": alias, "displayName": "A", "rootPath": root,
                    })))
                    .expect_err(alias)
                ),
                LocalErrorCode::InvalidParams,
                "alias={alias}"
            );
        }
        for display in [String::new(), "x".repeat(MAX_DISPLAY_NAME_CHARS + 1)] {
            assert_eq!(
                code_of(
                    workspace_select(&params(json!({
                        "alias": "a", "displayName": display, "rootPath": root,
                    })))
                    .expect_err(&display)
                ),
                LocalErrorCode::InvalidParams
            );
        }
        assert!(
            workspace_select(&params(json!({
                "alias": "a", "displayName": "A", "rootPath": root, "extra": 1,
            })))
            .is_err()
        );
        assert!(
            workspace_select(&params(json!({
                "alias": "a", "displayName": "A", "rootPath": root,
            })))
            .is_ok()
        );
        // `^[a-z0-9][a-z0-9._-]{0,63}$` 允许首字符是数字（权威：node-link 的 workspaceAlias）。
        assert!(
            workspace_select(&params(json!({
                "alias": "9alias", "displayName": "A", "rootPath": root,
            })))
            .is_ok()
        );
        fs::remove_dir_all(&directory).ok();
    }

    // ---------------------------------------------------------------- agent.configure

    #[test]
    fn agent_configure_requires_all_six_fields() {
        let complete = json!({
            "agentId": "codex",
            "displayName": "Codex",
            "command": "codex",
            "args": ["--acp"],
            "envAllowlist": ["OPENAI_API_KEY"],
            "default": true,
        });
        let configure = agent_configure(&params(complete.clone())).expect("完整参数必须通过");
        assert_eq!(configure.agent_id.as_str(), "codex");
        assert_eq!(configure.args, vec!["--acp".to_owned()]);
        assert_eq!(configure.env_allowlist, vec!["OPENAI_API_KEY".to_owned()]);
        assert!(configure.default);

        for missing in [
            "agentId",
            "displayName",
            "command",
            "args",
            "envAllowlist",
            "default",
        ] {
            let mut incomplete = complete.clone();
            incomplete.as_object_mut().expect("object").remove(missing);
            assert_eq!(
                code_of(agent_configure(&params(incomplete)).expect_err(missing)),
                LocalErrorCode::InvalidParams,
                "缺少 {missing}"
            );
        }

        for agent_id in [String::new(), "x".repeat(129)] {
            let mut wrong = complete.clone();
            wrong["agentId"] = json!(agent_id);
            assert_eq!(
                code_of(agent_configure(&params(wrong)).expect_err(&agent_id)),
                LocalErrorCode::InvalidParams
            );
        }

        let mut wrong_type = complete.clone();
        wrong_type["args"] = json!([1]);
        assert_eq!(
            code_of(agent_configure(&params(wrong_type)).expect_err("args 元素必须是字符串")),
            LocalErrorCode::InvalidParams
        );

        let mut unknown = complete;
        unknown["unknown"] = json!(1);
        assert_eq!(
            code_of(agent_configure(&params(unknown)).expect_err("未知字段")),
            LocalErrorCode::InvalidParams
        );
    }

    // ---------------------------------------------------------------- provider.configure

    #[test]
    fn provider_configure_validates_ids_fields_and_values() {
        let configure = provider_configure(&params(json!({
            "providerId": "openai.primary",
            "kind": "provider",
            "displayName": "OpenAI",
            "values": { "api_key": "s3cret", "org": "acme" },
        })))
        .expect("合法参数必须通过");
        assert_eq!(configure.kind, ProviderRefKind::Provider);
        assert_eq!(
            configure.values,
            vec![
                ("api_key".to_owned(), "s3cret".to_owned()),
                ("org".to_owned(), "acme".to_owned()),
            ]
        );

        for rejected in [
            json!({ "providerId": "bad id", "kind": "provider", "displayName": "A", "values": {"k": "v"} }),
            json!({ "providerId": &"a".repeat(65), "kind": "provider", "displayName": "A", "values": {"k": "v"} }),
            json!({ "providerId": "p", "kind": "other", "displayName": "A", "values": {"k": "v"} }),
            json!({ "providerId": "p", "kind": "provider", "displayName": "", "values": {"k": "v"} }),
            json!({ "providerId": "p", "kind": "provider", "displayName": "A", "values": {} }),
            json!({ "providerId": "p", "kind": "provider", "displayName": "A", "values": {"bad field": "v"} }),
            json!({ "providerId": "p", "kind": "provider", "displayName": "A", "values": {"k": ""} }),
            json!({ "providerId": "p", "kind": "provider", "displayName": "A", "values": { "k": 1 } }),
            json!({ "providerId": "p", "kind": "mcp" }),
        ] {
            assert_eq!(
                code_of(
                    provider_configure(&params(rejected.clone())).expect_err(&rejected.to_string())
                ),
                LocalErrorCode::InvalidParams,
                "{rejected}"
            );
        }
        assert_eq!(
            provider_configure(&params(json!({
                "providerId": "p", "kind": "mcp", "displayName": "P", "values": { "k": "v" },
            })))
            .expect("mcp 也是合法 kind")
            .kind,
            ProviderRefKind::Mcp
        );
    }

    // ---------------------------------------------------------------- export.create

    fn valid_export_params() -> serde_json::Value {
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

    fn created_at() -> Timestamp {
        Timestamp::new(TS).expect("timestamp")
    }

    #[test]
    fn export_create_accepts_the_documented_shape() {
        let record =
            export_create(&params(valid_export_params()), created_at()).expect("合法参数必须通过");
        assert_eq!(record.export_id().as_str(), "export-1");
        assert_eq!(record.agent_ids().len(), 1);
        assert_eq!(record.workspace_aliases().len(), 1);
        assert_eq!(record.templates().len(), 1);
        assert!(record.templates()[0].params().is_empty());
        assert_eq!(record.scopes().len(), 1);
        assert!(!record.is_revoked());
        assert_eq!(record.created_at().as_str(), TS);
    }

    #[test]
    fn export_create_enforces_the_single_slice_rules() {
        let mut two_agents = valid_export_params();
        two_agents["agentIds"] = json!(["codex", "omp"]);
        let mut no_agent = valid_export_params();
        no_agent["agentIds"] = json!([]);
        let mut two_aliases = valid_export_params();
        two_aliases["workspaceAliases"] = json!([
            { "alias": "project", "displayName": "Project" },
            { "alias": "other", "displayName": "Other" },
        ]);
        let mut two_templates = valid_export_params();
        two_templates["templates"] = json!([
            { "templateId": "t1", "displayName": "T1", "workspaceAlias": "project", "params": [] },
            { "templateId": "t2", "displayName": "T2", "workspaceAlias": "project", "params": [] },
        ]);
        let mut with_params = valid_export_params();
        with_params["templates"][0]["params"] = json!([
            { "name": "task", "type": "string", "required": true, "pattern": null, "enum": null },
        ]);
        let mut bad_cache_policy = valid_export_params();
        bad_cache_policy["cachePolicy"] = json!("content-cache");
        let mut unknown_alias = valid_export_params();
        unknown_alias["defaultWorkspaceAlias"] = json!("other");
        let mut unknown_template = valid_export_params();
        unknown_template["defaultTemplateId"] = json!("other");
        let mut template_alias_mismatch = valid_export_params();
        template_alias_mismatch["templates"][0]["workspaceAlias"] = json!("other");
        let mut bad_scope = valid_export_params();
        bad_scope["scopes"] = json!(["session.read"]);
        let mut unknown_field = valid_export_params();
        unknown_field["extra"] = json!(1);
        let mut missing_field = valid_export_params();
        missing_field
            .as_object_mut()
            .expect("object")
            .remove("scopes");

        for (label, rejected) in [
            ("agentIds 恰好 1 项", two_agents),
            ("agentIds 非空", no_agent),
            ("workspaceAliases 恰好 1 项", two_aliases),
            ("templates 恰好 1 项", two_templates),
            ("params 必须为空", with_params),
            ("cachePolicy 固定值", bad_cache_policy),
            ("defaultWorkspaceAlias 一致性", unknown_alias),
            ("defaultTemplateId 一致性", unknown_template),
            (
                "template 的 alias 必须在 aliases 内",
                template_alias_mismatch,
            ),
            ("scopes 必须是 grant.*", bad_scope),
            ("未知字段", unknown_field),
            ("缺字段", missing_field),
        ] {
            assert_eq!(
                code_of(
                    export_create(&params(rejected.clone()), created_at())
                        .expect_err(&format!("{label}: {rejected}"))
                ),
                LocalErrorCode::InvalidParams,
                "{label}"
            );
        }
    }

    #[test]
    fn export_revoke_and_import_remove_validate_the_id_shape() {
        assert_eq!(
            export_revoke(&params(json!({"exportId": "export-1"})))
                .expect("合法")
                .as_str(),
            "export-1"
        );
        assert_eq!(
            import_remove(&params(json!({"importId": "import-1"})))
                .expect("合法")
                .as_str(),
            "import-1"
        );
        for rejected in [json!({}), json!({"exportId": ""}), json!({"exportId": 1})] {
            assert_eq!(
                code_of(export_revoke(&params(rejected.clone())).expect_err("必须被拒")),
                LocalErrorCode::InvalidParams,
                "{rejected}"
            );
        }
        for rejected in [
            json!({}),
            json!({"importId": "bad id"}),
            json!({"importId": null}),
        ] {
            assert_eq!(
                code_of(import_remove(&params(rejected.clone())).expect_err("必须被拒")),
                LocalErrorCode::InvalidParams,
                "{rejected}"
            );
        }
    }

    // ---------------------------------------------------------------- audit.export

    #[test]
    fn audit_export_parses_filters_and_rejects_bad_shapes() {
        let parsed = audit_export(&params(json!({
            "outputPath": "D:\\audit\\export.jsonl",
            "format": "jsonl",
            "since": TS,
            "until": null,
            "categories": ["device.revoked", "provider.configured", "device.revoked"],
        })))
        .expect("合法参数必须通过");
        assert_eq!(parsed.format, AuditFormat::Jsonl);
        assert_eq!(parsed.since.expect("since").as_str(), TS);
        assert_eq!(parsed.until, None);
        assert_eq!(
            parsed.categories,
            vec![AuditAction::DeviceRevoked, AuditAction::ProviderConfigured],
            "类别去重且保序"
        );

        let empty_categories = audit_export(&params(json!({
            "outputPath": "D:\\audit\\export.csv",
            "format": "csv",
            "since": null,
            "until": null,
            "categories": [],
        })))
        .expect("空类别数组合法");
        assert!(empty_categories.categories.is_empty());

        for rejected in [
            json!({ "outputPath": "relative/path.jsonl", "format": "jsonl", "since": null, "until": null, "categories": [] }),
            json!({ "outputPath": "D:\\a.jsonl", "format": "yaml", "since": null, "until": null, "categories": [] }),
            json!({ "outputPath": "D:\\a.jsonl", "format": "jsonl", "since": "2026-09-18", "until": null, "categories": [] }),
            json!({ "outputPath": "D:\\a.jsonl", "format": "jsonl", "since": null, "until": null, "categories": ["nope"] }),
            json!({ "outputPath": "D:\\a.jsonl", "format": "jsonl", "since": null, "until": null }),
            json!({ "outputPath": "D:\\a.jsonl", "format": "jsonl", "since": null, "until": null, "categories": [], "extra": 1 }),
        ] {
            assert_eq!(
                code_of(audit_export(&params(rejected.clone())).expect_err(&rejected.to_string())),
                LocalErrorCode::InvalidParams,
                "{rejected}"
            );
        }
        assert_eq!(AuditFormat::parse("jsonl"), Some(AuditFormat::Jsonl));
        assert_eq!(AuditFormat::parse("csv"), Some(AuditFormat::Csv));
        assert_eq!(AuditFormat::parse("yaml"), None);
    }

    // ---------------------------------------------------------------- PortError 映射

    #[test]
    fn port_errors_map_to_the_documented_local_codes() {
        let cases: Vec<(&str, PortError, LocalErrorCode)> = vec![
            (
                "not_found",
                PortError::NotFound(acp_core::model::EntityRef::Export(
                    ExportId::new("export-1").expect("export id"),
                )),
                LocalErrorCode::NotFound,
            ),
            (
                "conflict",
                PortError::Conflict(ConflictKind::AlreadyExists),
                LocalErrorCode::Conflict,
            ),
            (
                "expired",
                PortError::Conflict(ConflictKind::Expired),
                LocalErrorCode::Expired,
            ),
            (
                "unavailable",
                PortError::Unavailable(acp_core::model::UnavailableKind::KeystoreUnavailable),
                LocalErrorCode::Unavailable,
            ),
            (
                "invalid_request",
                PortError::InvalidRequest("authorization.scope_denied"),
                LocalErrorCode::InvalidParams,
            ),
            (
                "corrupt",
                PortError::Corrupt("owned_workspace row is malformed"),
                LocalErrorCode::Internal,
            ),
            (
                "backend",
                PortError::Backend(Box::new(std::io::Error::other(
                    "SELECT * FROM owned_workspace failed at D:\\secret\\db.sqlite",
                ))),
                LocalErrorCode::Internal,
            ),
        ];
        for (label, error, expected) in cases {
            let mapped = map_port_error("export.create", error);
            assert_eq!(mapped.code(), expected, "{label}");
            assert!(mapped.message().starts_with("export.create: "), "{label}");
            assert!(mapped.message().len() <= 512, "{label}");
        }

        for kind in ConflictKind::ALL {
            let expected = if kind == ConflictKind::Expired {
                LocalErrorCode::Expired
            } else {
                LocalErrorCode::Conflict
            };
            assert_eq!(
                map_port_error("export.list", PortError::Conflict(kind)).code(),
                expected,
                "{kind}"
            );
        }
        for kind in acp_core::model::UnavailableKind::ALL {
            assert_eq!(
                map_port_error("export.list", PortError::Unavailable(kind)).code(),
                LocalErrorCode::Unavailable,
                "{kind}"
            );
        }

        // 适配器文本（SQL 与完整路径）不得被转述到消息里。
        let leaky = map_port_error(
            "export.list",
            PortError::Backend(Box::new(std::io::Error::other(
                "SELECT * FROM owned_export; D:\\secret\\db.sqlite",
            ))),
        );
        assert!(!leaky.message().contains("SQL"), "{}", leaky.message());
        assert!(!leaky.message().contains("SELECT"), "{}", leaky.message());
        assert!(
            !leaky.message().contains("db.sqlite"),
            "{}",
            leaky.message()
        );
    }
}
