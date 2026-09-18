//! Export 与 Import 记录（`docs/CORE_PORTS_AND_STORAGE.md` §3.5，形状来源 `LOCAL_ADMIN_PROTOCOL.md` §5.5）。
//!
//! Export 是 Owner 侧「发布给 Access Node 的受策略约束资源集合」，Import 是 Access 侧对该 Export 的引用。
//! 两者都只描述**策略与引用**，不含任何会话正文；`cachePolicy` 在 v1 只有 `no-content-cache`。

use super::error::InvalidValue;
use super::identity::GrantSet;
use super::ids::{
    AgentId, ExportId, ImportId, NodeId, TemplateId, WorkspaceAlias, is_endpoint, is_template_id,
    require_bounded,
};
use super::scalars::Timestamp;
use std::fmt;
use std::str::FromStr;

token_enum!(
    /// Export/Import 的缓存策略；v1 固定值（`NODE_LINK_PROTOCOL.md` §10）。
    CachePolicy {
        NoContentCache => "no-content-cache",
    }
);

newtype!(
    /// template 内参数名：`^[A-Za-z0-9._-]{1,64}$`。
    ParamName,
    check_param_name
);

fn check_param_name(text: &str) -> Result<(), InvalidValue> {
    if is_template_id(text) {
        Ok(())
    } else {
        Err(InvalidValue::TemplateId)
    }
}

/// workspace alias 与展示名（`workspaceAliases[]`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAliasEntry {
    alias: WorkspaceAlias,
    display_name: String,
}

impl WorkspaceAliasEntry {
    /// 构造。`display_name` 1..=128 字符。
    pub fn try_new(alias: WorkspaceAlias, display_name: &str) -> Result<Self, InvalidValue> {
        require_bounded(display_name, 1, 128)?;
        Ok(Self {
            alias,
            display_name: display_name.to_owned(),
        })
    }

    /// 符号名（不构成路径）。
    pub fn alias(&self) -> &WorkspaceAlias {
        &self.alias
    }

    /// 展示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

token_enum!(
    /// template 参数类型（`LOCAL_ADMIN_PROTOCOL.md` §5.5 的 `TemplateParam.type`）。
    TemplateParamType {
        String => "string",
        Boolean => "boolean",
        Integer => "integer",
    }
);

/// template 参数的可选枚举成员（成员类型必须与参数的 `type` 一致）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateParamValue {
    String(String),
    Boolean(bool),
    Integer(i64),
}

impl TemplateParamValue {
    /// 成员自身的类型。
    pub fn ty(&self) -> TemplateParamType {
        match self {
            Self::String(_) => TemplateParamType::String,
            Self::Boolean(_) => TemplateParamType::Boolean,
            Self::Integer(_) => TemplateParamType::Integer,
        }
    }
}

/// Export template 的参数声明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateParam {
    name: ParamName,
    ty: TemplateParamType,
    required: bool,
    pattern: Option<String>,
    enum_values: Option<Vec<TemplateParamValue>>,
}

impl TemplateParam {
    /// 构造。`pattern` 只允许出现在 `type = string`；`enum_values` 必须非空且成员类型与 `type` 一致。
    pub fn try_new(
        name: ParamName,
        ty: TemplateParamType,
        required: bool,
        pattern: Option<String>,
        enum_values: Option<Vec<TemplateParamValue>>,
    ) -> Result<Self, InvalidValue> {
        if let Some(pattern) = &pattern {
            if ty != TemplateParamType::String {
                return Err(InvalidValue::Field);
            }
            require_bounded(pattern, 1, 4096)?;
        }
        if let Some(values) = &enum_values {
            if values.is_empty() {
                return Err(InvalidValue::Empty);
            }
            if values.iter().any(|value| value.ty() != ty) {
                return Err(InvalidValue::Field);
            }
        }
        Ok(Self {
            name,
            ty,
            required,
            pattern,
            enum_values,
        })
    }

    /// 参数名。
    pub fn name(&self) -> &ParamName {
        &self.name
    }

    /// 参数类型。
    pub fn ty(&self) -> TemplateParamType {
        self.ty
    }

    /// 是否必填。
    pub fn required(&self) -> bool {
        self.required
    }

    /// 仅 `type = string` 时的正则约束。
    pub fn pattern(&self) -> Option<&str> {
        self.pattern.as_deref()
    }

    /// 可选枚举成员。
    pub fn enum_values(&self) -> Option<&[TemplateParamValue]> {
        self.enum_values.as_deref()
    }
}

/// Export template（`LOCAL_ADMIN_PROTOCOL.md` §5.5 的 `ExportTemplate`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportTemplate {
    template_id: TemplateId,
    display_name: String,
    workspace_alias: WorkspaceAlias,
    params: Vec<TemplateParam>,
}

impl ExportTemplate {
    /// 构造。`display_name` 1..=128 字符；参数名不得重复。
    pub fn try_new(
        template_id: TemplateId,
        display_name: &str,
        workspace_alias: WorkspaceAlias,
        params: Vec<TemplateParam>,
    ) -> Result<Self, InvalidValue> {
        require_bounded(display_name, 1, 128)?;
        for (index, param) in params.iter().enumerate() {
            if params[..index]
                .iter()
                .any(|other| other.name() == param.name())
            {
                return Err(InvalidValue::Field);
            }
        }
        Ok(Self {
            template_id,
            display_name: display_name.to_owned(),
            workspace_alias,
            params,
        })
    }

    /// template 标识。
    pub fn template_id(&self) -> &TemplateId {
        &self.template_id
    }

    /// 展示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 绑定的 workspace alias（必须是所属 Export 的 aliases 之一）。
    pub fn workspace_alias(&self) -> &WorkspaceAlias {
        &self.workspace_alias
    }

    /// 参数声明。
    pub fn params(&self) -> &[TemplateParam] {
        &self.params
    }
}

/// Export 记录（§3.5，形状与 `LOCAL_ADMIN_PROTOCOL.md` §5.5 的 `ExportView` 一致）。
///
/// 不变式：`agent_ids`/`workspace_aliases`/`templates` 非空且各自不重复；
/// `default_workspace_alias` 必须是 `workspace_aliases` 之一；`default_template_id` 必须是 `templates` 之一；
/// 每个 template 的 `workspace_alias` 必须已声明。撤销状态由 `revoked_at` 表达（`export.list` 包含已撤销项）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRecord {
    export_id: ExportId,
    display_name: String,
    agent_ids: Vec<AgentId>,
    workspace_aliases: Vec<WorkspaceAliasEntry>,
    default_workspace_alias: WorkspaceAlias,
    templates: Vec<ExportTemplate>,
    default_template_id: TemplateId,
    scopes: GrantSet,
    cache_policy: CachePolicy,
    created_at: Timestamp,
    revoked_at: Option<Timestamp>,
}

impl ExportRecord {
    /// 构造。详见类型级文档的不变式。
    #[allow(clippy::too_many_arguments)] // 与 §5.5 的 `ExportView` 字段一一对应
    pub fn try_new(
        export_id: ExportId,
        display_name: &str,
        agent_ids: Vec<AgentId>,
        workspace_aliases: Vec<WorkspaceAliasEntry>,
        default_workspace_alias: WorkspaceAlias,
        templates: Vec<ExportTemplate>,
        default_template_id: TemplateId,
        scopes: GrantSet,
        cache_policy: CachePolicy,
        created_at: Timestamp,
        revoked_at: Option<Timestamp>,
    ) -> Result<Self, InvalidValue> {
        require_bounded(display_name, 1, 128)?;
        if agent_ids.is_empty() {
            return Err(InvalidValue::Empty);
        }
        for (index, agent) in agent_ids.iter().enumerate() {
            if agent_ids[..index].contains(agent) {
                return Err(InvalidValue::Field);
            }
        }
        if workspace_aliases.is_empty() || templates.is_empty() {
            return Err(InvalidValue::Empty);
        }
        for (index, entry) in workspace_aliases.iter().enumerate() {
            if workspace_aliases[..index]
                .iter()
                .any(|other| other.alias() == entry.alias())
            {
                return Err(InvalidValue::Field);
            }
        }
        if !workspace_aliases
            .iter()
            .any(|entry| entry.alias() == &default_workspace_alias)
        {
            return Err(InvalidValue::Field);
        }
        for (index, template) in templates.iter().enumerate() {
            if templates[..index]
                .iter()
                .any(|other| other.template_id() == template.template_id())
            {
                return Err(InvalidValue::Field);
            }
            if !workspace_aliases
                .iter()
                .any(|entry| entry.alias() == template.workspace_alias())
            {
                return Err(InvalidValue::Field);
            }
        }
        if !templates
            .iter()
            .any(|template| template.template_id() == &default_template_id)
        {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            export_id,
            display_name: display_name.to_owned(),
            agent_ids,
            workspace_aliases,
            default_workspace_alias,
            templates,
            default_template_id,
            scopes,
            cache_policy,
            created_at,
            revoked_at,
        })
    }

    /// Export 标识。
    pub fn export_id(&self) -> &ExportId {
        &self.export_id
    }

    /// 展示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Export 内的 Agent selector。
    pub fn agent_ids(&self) -> &[AgentId] {
        &self.agent_ids
    }

    /// workspace alias 声明。
    pub fn workspace_aliases(&self) -> &[WorkspaceAliasEntry] {
        &self.workspace_aliases
    }

    /// 默认 workspace alias。
    pub fn default_workspace_alias(&self) -> &WorkspaceAlias {
        &self.default_workspace_alias
    }

    /// template 声明。
    pub fn templates(&self) -> &[ExportTemplate] {
        &self.templates
    }

    /// 默认 template。
    pub fn default_template_id(&self) -> &TemplateId {
        &self.default_template_id
    }

    /// `grant.*` 上限。
    pub fn scopes(&self) -> &GrantSet {
        &self.scopes
    }

    /// 缓存策略（v1 固定 `no-content-cache`）。
    pub fn cache_policy(&self) -> CachePolicy {
        self.cache_policy
    }

    /// 创建时间。
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }

    /// 撤销时间；`None` 表示仍然有效。
    pub fn revoked_at(&self) -> Option<&Timestamp> {
        self.revoked_at.as_ref()
    }

    /// 是否已撤销。
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }
}

/// Import 记录（§3.5，形状与 `CONFIG_REFERENCE.md` §9 的 `[[imports]]` 一致）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    import_id: ImportId,
    owner_endpoint: String,
    owner_node_id: NodeId,
    export_ids: Vec<ExportId>,
    grants: GrantSet,
}

impl ImportRecord {
    /// 构造。`owner_endpoint` 必须是 `wss://` 端点；`export_ids` 非空且不重复。
    pub fn try_new(
        import_id: ImportId,
        owner_endpoint: &str,
        owner_node_id: NodeId,
        export_ids: Vec<ExportId>,
        grants: GrantSet,
    ) -> Result<Self, InvalidValue> {
        if !is_endpoint(owner_endpoint) {
            return Err(InvalidValue::Endpoint);
        }
        if export_ids.is_empty() {
            return Err(InvalidValue::Empty);
        }
        for (index, export) in export_ids.iter().enumerate() {
            if export_ids[..index].contains(export) {
                return Err(InvalidValue::Field);
            }
        }
        Ok(Self {
            import_id,
            owner_endpoint: owner_endpoint.to_owned(),
            owner_node_id,
            export_ids,
            grants,
        })
    }

    /// Import 标识。
    pub fn import_id(&self) -> &ImportId {
        &self.import_id
    }

    /// Owner 的 `wss://` 端点。
    pub fn owner_endpoint(&self) -> &str {
        &self.owner_endpoint
    }

    /// Owner 节点标识。
    pub fn owner_node_id(&self) -> &NodeId {
        &self.owner_node_id
    }

    /// 引用的 Export 集合。
    pub fn export_ids(&self) -> &[ExportId] {
        &self.export_ids
    }

    /// 本地 `grant.*` 上限。
    pub fn grants(&self) -> &GrantSet {
        &self.grants
    }
}
