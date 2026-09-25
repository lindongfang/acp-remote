//! core 值对象 → 管理 `result` 的投影（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.2/§5.5）。
//!
//! 只做形状投影：字段名一律 `camelCase`（§1.1），时间戳是 §1.1 的 RFC 3339 毫秒文本，数组保序。
//! 这里**不**做校验（校验在 [`crate::local_admin::params`]）、不读时钟、不接触存储；导出给
//! `result` 的字段集合与 §5.2/§5.5 的表格逐项对应，未知字段不会被凭空造出来。

use acp_core::model::{
    AgentProfile, ExportRecord, ImportRecord, TemplateParam, TemplateParamValue, Timestamp,
    WorkspaceRecord,
};
use serde_json::Value;

use crate::local_admin::envelope::JsonObject;

/// 由键值对构造一个 JSON object（保序按调用方给出的顺序）。
pub(crate) fn object(entries: Vec<(&str, Value)>) -> JsonObject {
    let mut map = JsonObject::new();
    for (key, value) in entries {
        map.insert(key.to_owned(), value);
    }
    map
}

/// 字符串值（`impl AsRef<str>` 覆盖 `&str`/`String`/`&String`）。
pub(crate) fn text(value: impl AsRef<str>) -> Value {
    Value::String(value.as_ref().to_owned())
}

/// §1.1 的时间戳文本（UTC、毫秒、`Z`）。
pub(crate) fn timestamp(value: &Timestamp) -> Value {
    text(value.as_str())
}

/// 字符串数组。
pub(crate) fn string_array<'a>(items: impl IntoIterator<Item = &'a str>) -> Value {
    Value::Array(items.into_iter().map(text).collect())
}

/// `workspace.select` 的 result 内层（§5.2）：`{ alias, displayName, rootPath, createdAt }`。
pub(crate) fn workspace(record: &WorkspaceRecord) -> JsonObject {
    object(vec![
        ("alias", text(record.alias().as_str())),
        ("displayName", text(record.display_name())),
        ("rootPath", text(record.canonical_path())),
        ("createdAt", timestamp(record.created_at())),
    ])
}

/// `agent.configure` 的 result 内层（§5.2）：`{ agentId, displayName, default }`。
pub(crate) fn agent(profile: &AgentProfile) -> JsonObject {
    object(vec![
        ("agentId", text(profile.id().as_str())),
        ("displayName", text(profile.display_name())),
        ("default", Value::Bool(profile.is_default())),
    ])
}

/// `provider.configure` 的 result 内层（§5.2）：只回字段名，**永不回显 `values`**。
pub(crate) fn provider(provider_id: &str, kind: &str, configured_fields: &[String]) -> JsonObject {
    object(vec![
        ("providerId", text(provider_id)),
        ("kind", text(kind)),
        (
            "configuredFields",
            Value::Array(configured_fields.iter().map(text).collect()),
        ),
    ])
}

/// `export.create`／`export.list` 的 `ExportView`（§5.5）。
pub(crate) fn export(record: &ExportRecord) -> JsonObject {
    object(vec![
        ("exportId", text(record.export_id().as_str())),
        ("displayName", text(record.display_name())),
        (
            "agentIds",
            Value::Array(
                record
                    .agent_ids()
                    .iter()
                    .map(|id| text(id.as_str()))
                    .collect(),
            ),
        ),
        (
            "workspaceAliases",
            Value::Array(
                record
                    .workspace_aliases()
                    .iter()
                    .map(|entry| {
                        Value::Object(object(vec![
                            ("alias", text(entry.alias().as_str())),
                            ("displayName", text(entry.display_name())),
                        ]))
                    })
                    .collect(),
            ),
        ),
        (
            "defaultWorkspaceAlias",
            text(record.default_workspace_alias().as_str()),
        ),
        (
            "templates",
            Value::Array(
                record
                    .templates()
                    .iter()
                    .map(|template| {
                        Value::Object(object(vec![
                            ("templateId", text(template.template_id().as_str())),
                            ("displayName", text(template.display_name())),
                            ("workspaceAlias", text(template.workspace_alias().as_str())),
                            (
                                "params",
                                Value::Array(
                                    template
                                        .params()
                                        .iter()
                                        .map(|param| Value::Object(template_param(param)))
                                        .collect(),
                                ),
                            ),
                        ]))
                    })
                    .collect(),
            ),
        ),
        (
            "defaultTemplateId",
            text(record.default_template_id().as_str()),
        ),
        ("scopes", string_array(record.scopes().iter())),
        ("cachePolicy", text(record.cache_policy().as_str())),
        ("createdAt", timestamp(record.created_at())),
        (
            "revokedAt",
            match record.revoked_at() {
                Some(at) => timestamp(at),
                None => Value::Null,
            },
        ),
    ])
}

/// `import.list` 的 `ImportRecord`（§5.5，与 `CONFIG_REFERENCE.md` §9 的 `[[imports]]` 同名同义）。
pub(crate) fn import(record: &ImportRecord) -> JsonObject {
    object(vec![
        ("importId", text(record.import_id().as_str())),
        ("ownerEndpoint", text(record.owner_endpoint())),
        ("ownerNodeId", text(record.owner_node_id().as_str())),
        (
            "exportIds",
            Value::Array(
                record
                    .export_ids()
                    .iter()
                    .map(|id| text(id.as_str()))
                    .collect(),
            ),
        ),
        ("grants", string_array(record.grants().iter())),
    ])
}

fn template_param(param: &TemplateParam) -> JsonObject {
    object(vec![
        ("name", text(param.name().as_str())),
        ("type", text(param.ty().as_str())),
        ("required", Value::Bool(param.required())),
        (
            "pattern",
            match param.pattern() {
                Some(pattern) => text(pattern),
                None => Value::Null,
            },
        ),
        (
            "enum",
            match param.enum_values() {
                Some(values) => Value::Array(
                    values
                        .iter()
                        .map(|value| match value {
                            TemplateParamValue::String(text) => Value::String(text.clone()),
                            TemplateParamValue::Boolean(flag) => Value::Bool(*flag),
                            TemplateParamValue::Integer(number) => Value::from(*number),
                        })
                        .collect(),
                ),
                None => Value::Null,
            },
        ),
    ])
}
