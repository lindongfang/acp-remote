//! `ExportStore` 的 SQLite 实现（Export 写集、Import 管理行 + 关联行、完整移除）。
//!
//! 逐条对应 `docs/CORE_PORTS_AND_STORAGE.md` §11.6 第 7 条与 §7.4：`put_export`/`revoke_export`
//! 的审计取值是 `export.created`/`export.revoked`，`add_import`/`remove_import` 是 `import.added`/
//! `import.removed`，都与状态同事务。
//!
//! 两条删除权威不重叠（§11.6）：`remove_import`（本模块）做**完整移除**——管理行、关联行、
//! `imported_session` 及其级联（交付索引、命令引用）——而 `RemoteDeliveryStore::drop_import`
//! （`session_store`）只做连接级清空。`remove_import` **不删审计**。
//!
//! 管理行/关联行的字段来源（写集没有携带的列在这里如实取默认，不发明事实）：
//!
//! - `imported_import.endpoint_ref` 取 `ImportRecord.owner_endpoint`（模型保证是 `wss://` 端点）；
//! - `imported_import.cache_policy` 固定 `no-content-cache`（v1 唯一取值，§7.4 的 CHECK）；
//! - `imported_import.owner_server_epoch` 写 NULL：Owner 的 `serverEpoch` 来自 Node Link 握手，
//!   不在本写集里（未知即不写，避免伪造「已确认 epoch」）；
//! - `imported_import.created_at`/`imported_import_export.added_at` 取 `WriteContext.at`（`ImportRecord`
//!   不带时间戳，时间只由调用方的 `Clock` 提供，§2）；
//! - `imported_import.removed_at` 由 `RemoteDeliveryStore::drop_import`/`ImportRemoval` 之外的路径
//!   写入；`ImportRemoval` 是删除而非打标，因此这里保持 NULL。

use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;

use acp_core::model::{
    AgentId, ConflictKind, EntityRef, ExportRecord, ExportTemplate, GrantSet, ImportId,
    ImportRecord, ParamName, PortError, TemplateParam, TemplateParamType, TemplateParamValue,
    Timestamp, WorkspaceAliasEntry,
};
use acp_core::ports::{ExportRevocation, ExportStore, ExportWrite, ImportRemoval, ImportWrite};

use crate::admin::{
    AliasJson, ParamJson, TemplateJson, decode_strings, decode_value, encode_strings, encode_value,
    enforce_capacity_gate, insert_audit_rows,
};
use crate::error::StorageError;
use crate::session_store::{Db, SqliteStore, decode, decode_opt, opt_text, text};

/// `owned_export` 的读列。
const EXPORT_COLUMNS: &str = "export_id, display_name, agent_ids_json, aliases_json, default_alias, \
     templates_json, default_template, scopes_json, cache_policy, created_at, revoked_at";

/// `imported_import` 的读列。
const IMPORT_COLUMNS: &str = "import_id, owner_node_id, endpoint_ref, grants_json";

// ---------------------------------------------------------------------------------------------
// 行 → 值对象
// ---------------------------------------------------------------------------------------------

fn export_from_row(row: &SqliteRow) -> Result<ExportRecord, StorageError> {
    let agent_ids = decode_strings(
        &text(row, "agent_ids_json")?,
        "owned_export.agent_ids_json is not a JSON array",
    )?
    .into_iter()
    .map(|id| decode(&id, "owned_export.agent_ids_json"))
    .collect::<Result<Vec<AgentId>, _>>()?;
    let aliases = decode_value::<Vec<AliasJson>>(
        &text(row, "aliases_json")?,
        "owned_export.aliases_json is not a JSON array of {alias,displayName}",
    )?
    .into_iter()
    .map(|entry| {
        WorkspaceAliasEntry::try_new(
            decode(&entry.alias, "owned_export.aliases_json.alias")?,
            &entry.display_name,
        )
        .map_err(StorageError::from)
    })
    .collect::<Result<Vec<_>, _>>()?;
    let templates = decode_value::<Vec<TemplateJson>>(
        &text(row, "templates_json")?,
        "owned_export.templates_json is not a JSON array of templates",
    )?
    .into_iter()
    .map(template_from_json)
    .collect::<Result<Vec<_>, _>>()?;
    ExportRecord::try_new(
        decode(&text(row, "export_id")?, "owned_export.export_id")?,
        &text(row, "display_name")?,
        agent_ids,
        aliases,
        decode(&text(row, "default_alias")?, "owned_export.default_alias")?,
        templates,
        decode(
            &text(row, "default_template")?,
            "owned_export.default_template",
        )?,
        GrantSet::try_from_iter(decode_strings(
            &text(row, "scopes_json")?,
            "owned_export.scopes_json is not a JSON array",
        )?)?,
        decode(&text(row, "cache_policy")?, "owned_export.cache_policy")?,
        decode(&text(row, "created_at")?, "owned_export.created_at")?,
        decode_opt(opt_text(row, "revoked_at")?, "owned_export.revoked_at")?,
    )
    .map_err(StorageError::from)
}

fn template_from_json(template: TemplateJson) -> Result<ExportTemplate, StorageError> {
    let params = template
        .params
        .into_iter()
        .map(param_from_json)
        .collect::<Result<Vec<_>, _>>()?;
    ExportTemplate::try_new(
        decode(
            &template.template_id,
            "owned_export.templates_json.template_id",
        )?,
        &template.display_name,
        decode(
            &template.workspace_alias,
            "owned_export.templates_json.workspace_alias",
        )?,
        params,
    )
    .map_err(StorageError::from)
}

fn param_from_json(param: ParamJson) -> Result<TemplateParam, StorageError> {
    let ty = decode::<TemplateParamType>(&param.ty, "owned_export.templates_json.params.type")?;
    let enum_values = param
        .enum_values
        .map(|values| {
            values
                .into_iter()
                .map(param_value)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    TemplateParam::try_new(
        decode::<ParamName>(&param.name, "owned_export.templates_json.params.name")?,
        ty,
        param.required,
        param.pattern,
        enum_values,
    )
    .map_err(StorageError::from)
}

/// `enumValues` 的三种线格式（`TemplateParamValue` 只有 String/Boolean/Integer）。
fn param_value(value: serde_json::Value) -> Result<TemplateParamValue, StorageError> {
    match value {
        serde_json::Value::String(text) => Ok(TemplateParamValue::String(text)),
        serde_json::Value::Bool(flag) => Ok(TemplateParamValue::Boolean(flag)),
        serde_json::Value::Number(number) => number
            .as_i64()
            .map(TemplateParamValue::Integer)
            .ok_or(StorageError::Corrupt(
                "owned_export.templates_json.params.enumValues is not an integer",
            )),
        _ => Err(StorageError::Corrupt(
            "owned_export.templates_json.params.enumValues has an unsupported shape",
        )),
    }
}

fn param_value_json(value: &TemplateParamValue) -> serde_json::Value {
    match value {
        TemplateParamValue::String(text) => serde_json::Value::String(text.clone()),
        TemplateParamValue::Boolean(flag) => serde_json::Value::Bool(*flag),
        TemplateParamValue::Integer(number) => serde_json::Value::Number((*number).into()),
    }
}

fn export_templates_json(record: &ExportRecord) -> Result<String, StorageError> {
    let templates: Vec<TemplateJson> = record
        .templates()
        .iter()
        .map(|template| TemplateJson {
            template_id: template.template_id().as_str().to_owned(),
            display_name: template.display_name().to_owned(),
            workspace_alias: template.workspace_alias().as_str().to_owned(),
            params: template
                .params()
                .iter()
                .map(|param| ParamJson {
                    name: param.name().as_str().to_owned(),
                    ty: param.ty().as_str().to_owned(),
                    required: param.required(),
                    pattern: param.pattern().map(str::to_owned),
                    enum_values: param
                        .enum_values()
                        .map(|values| values.iter().map(param_value_json).collect()),
                })
                .collect(),
        })
        .collect();
    encode_value(&templates, "export templates are not encodable")
}

fn export_aliases_json(record: &ExportRecord) -> Result<String, StorageError> {
    let aliases: Vec<AliasJson> = record
        .workspace_aliases()
        .iter()
        .map(|entry| AliasJson {
            alias: entry.alias().as_str().to_owned(),
            display_name: entry.display_name().to_owned(),
        })
        .collect();
    encode_value(&aliases, "export aliases are not encodable")
}

/// 关联表里的 Export 集合（按 export_id 升序，读路径稳定）。
async fn import_exports<'e, E>(executor: E, import_id: &str) -> Result<Vec<String>, StorageError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query_scalar(
        "SELECT export_id FROM imported_import_export WHERE import_id = ?1 ORDER BY export_id ASC",
    )
    .bind(import_id)
    .fetch_all(executor)
    .await
    .map_err(StorageError::from)
}

fn import_from_row(row: &SqliteRow, export_ids: Vec<String>) -> Result<ImportRecord, StorageError> {
    let export_ids = export_ids
        .into_iter()
        .map(|id| decode(&id, "imported_import_export.export_id"))
        .collect::<Result<Vec<_>, _>>()?;
    let endpoint = opt_text(row, "endpoint_ref")?.ok_or(StorageError::ColumnValue {
        column: "imported_import.endpoint_ref",
        expected: "wss:// endpoint",
    })?;
    ImportRecord::try_new(
        decode(&text(row, "import_id")?, "imported_import.import_id")?,
        &endpoint,
        decode(
            &text(row, "owner_node_id")?,
            "imported_import.owner_node_id",
        )?,
        export_ids,
        GrantSet::try_from_iter(decode_strings(
            &text(row, "grants_json")?,
            "imported_import.grants_json is not a JSON array",
        )?)?,
    )
    .map_err(StorageError::from)
}

/// 关联对 `(owner_node_id, export_id)`：完整移除时用来定位交付索引与会话行。
async fn import_pairs<'e, E>(
    executor: E,
    import_id: &str,
) -> Result<Vec<(String, String)>, StorageError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query_as(
        "SELECT owner_node_id, export_id FROM imported_import_export WHERE import_id = ?1 \
         ORDER BY export_id ASC",
    )
    .bind(import_id)
    .fetch_all(executor)
    .await
    .map_err(StorageError::from)
}

#[async_trait]
impl ExportStore for SqliteStore {
    async fn export(
        &self,
        id: &acp_core::model::ExportId,
    ) -> Result<Option<ExportRecord>, PortError> {
        let sql = format!("SELECT {EXPORT_COLUMNS} FROM owned_export WHERE export_id = ?1");
        let row = sqlx::query(&sql)
            .bind(id.as_str())
            .fetch_optional(&self.pools().read)
            .await
            .db()?;
        Ok(row.as_ref().map(export_from_row).transpose()?)
    }

    async fn exports(&self) -> Result<Vec<ExportRecord>, PortError> {
        let sql = format!(
            "SELECT {EXPORT_COLUMNS} FROM owned_export ORDER BY created_at ASC, export_id ASC"
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pools().read).await.db()?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            records.push(export_from_row(row)?);
        }
        Ok(records)
    }

    async fn import(&self, id: &ImportId) -> Result<Option<ImportRecord>, PortError> {
        let sql = format!("SELECT {IMPORT_COLUMNS} FROM imported_import WHERE import_id = ?1");
        let row = sqlx::query(&sql)
            .bind(id.as_str())
            .fetch_optional(&self.pools().read)
            .await
            .db()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let exports = import_exports(&self.pools().read, id.as_str()).await?;
        Ok(Some(import_from_row(&row, exports)?))
    }

    async fn imports(&self) -> Result<Vec<ImportRecord>, PortError> {
        let sql = format!("SELECT {IMPORT_COLUMNS} FROM imported_import ORDER BY import_id ASC");
        let rows = sqlx::query(&sql).fetch_all(&self.pools().read).await.db()?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            let import_id = text(row, "import_id")?;
            let exports = import_exports(&self.pools().read, &import_id).await?;
            records.push(import_from_row(row, exports)?);
        }
        Ok(records)
    }

    /// §11.6 第 7 条：写一个 Export（状态 + 集合 + 审计同事务）。
    async fn put_export(&self, write: ExportWrite) -> Result<(), PortError> {
        self.writable()?;
        let record = write.record;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        sqlx::query(
            "INSERT INTO owned_export (export_id, display_name, agent_ids_json, aliases_json, \
             default_alias, templates_json, default_template, scopes_json, cache_policy, created_at, \
             revoked_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
             ON CONFLICT(export_id) DO UPDATE SET display_name = excluded.display_name, \
             agent_ids_json = excluded.agent_ids_json, aliases_json = excluded.aliases_json, \
             default_alias = excluded.default_alias, templates_json = excluded.templates_json, \
             default_template = excluded.default_template, scopes_json = excluded.scopes_json, \
             cache_policy = excluded.cache_policy, revoked_at = excluded.revoked_at",
        )
        .bind(record.export_id().as_str())
        .bind(record.display_name())
        .bind(encode_strings(
            record.agent_ids().iter().map(|id| id.as_str().to_owned()),
        ))
        .bind(export_aliases_json(&record)?)
        .bind(record.default_workspace_alias().as_str())
        .bind(export_templates_json(&record)?)
        .bind(record.default_template_id().as_str())
        .bind(encode_strings(record.scopes().iter().map(str::to_owned)))
        .bind(record.cache_policy().as_str())
        .bind(record.created_at().as_str())
        .bind(record.revoked_at().map(Timestamp::as_str))
        .execute(&mut *tx)
        .await
        .db()
        .map_err(|error| error.into_conflict(ConflictKind::AlreadyExists))?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    /// §11.6 第 7 条 + §11.2 第 4 条：撤销先提交，再由调用方发送 `export.revoked`。重复撤销幂等。
    async fn revoke_export(&self, write: ExportRevocation) -> Result<(), PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM owned_export WHERE export_id = ?1")
                .bind(write.export.as_str())
                .fetch_optional(&mut *tx)
                .await
                .db()?;
        if exists.is_none() {
            return Err(PortError::NotFound(EntityRef::Export(write.export.clone())));
        }
        sqlx::query(
            "UPDATE owned_export SET revoked_at = COALESCE(revoked_at, ?2) WHERE export_id = ?1",
        )
        .bind(write.export.as_str())
        .bind(write.context.at.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    /// §11.6 第 7 条：管理行 + 全部关联行一次提交；重复 ID 或重复归属显式冲突，不静默合并。
    async fn add_import(&self, write: ImportWrite) -> Result<(), PortError> {
        self.writable()?;
        let record = write.record;
        // `exports` 与 `record.export_ids()` 必须一致（§5.3 的 `ImportWrite` 约束）：两处集合各自
        // 漂移会让管理行与关联行不一致，因此在这里失败关闭。
        if !same_set(&write.exports, record.export_ids()) {
            return Err(PortError::InvalidRequest(
                "ImportWrite.exports must equal record.export_ids()",
            ));
        }
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM imported_import WHERE import_id = ?1")
                .bind(record.import_id().as_str())
                .fetch_optional(&mut *tx)
                .await
                .db()?;
        if existing.is_some() {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        for export_id in record.export_ids() {
            let taken: Option<String> = sqlx::query_scalar(
                "SELECT import_id FROM imported_import_export \
                 WHERE owner_node_id = ?1 AND export_id = ?2",
            )
            .bind(record.owner_node_id().as_str())
            .bind(export_id.as_str())
            .fetch_optional(&mut *tx)
            .await
            .db()?;
            if taken.is_some() {
                return Err(PortError::Conflict(ConflictKind::DuplicateOwnership));
            }
        }
        sqlx::query(
            "INSERT INTO imported_import (import_id, owner_node_id, endpoint_ref, display_name, \
             cache_policy, owner_server_epoch, created_at, removed_at, grants_json) \
             VALUES (?1, ?2, ?3, NULL, 'no-content-cache', NULL, ?4, NULL, ?5)",
        )
        .bind(record.import_id().as_str())
        .bind(record.owner_node_id().as_str())
        .bind(record.owner_endpoint())
        .bind(write.context.at.as_str())
        .bind(encode_strings(record.grants().iter().map(str::to_owned)))
        .execute(&mut *tx)
        .await
        .db()
        .map_err(|error| error.into_conflict(ConflictKind::AlreadyExists))?;
        for export_id in record.export_ids() {
            sqlx::query(
                "INSERT INTO imported_import_export (import_id, owner_node_id, export_id, added_at) \
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(record.import_id().as_str())
            .bind(record.owner_node_id().as_str())
            .bind(export_id.as_str())
            .bind(write.context.at.as_str())
            .execute(&mut *tx)
            .await
            .db()
            .map_err(|error| error.into_conflict(ConflictKind::DuplicateOwnership))?;
        }
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    /// §11.6 第 7 条：完整移除——管理行 + 关联行 + `imported_session` 及其级联（交付索引、命令
    /// 引用），**审计保留**。
    async fn remove_import(&self, write: ImportRemoval) -> Result<(), PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM imported_import WHERE import_id = ?1")
                .bind(write.import.as_str())
                .fetch_optional(&mut *tx)
                .await
                .db()?;
        if exists.is_none() {
            return Err(PortError::NotFound(EntityRef::Import(write.import.clone())));
        }
        // 先取关联对（删管理行会经外键级联删掉关联行，之后就没法定位会话行了）。
        let pairs = import_pairs(&mut *tx, write.import.as_str()).await?;
        for (owner_node_id, export_id) in &pairs {
            // `imported_session` 是交付索引与命令引用的父表（`ON DELETE CASCADE`），因此这一步
            // 同时清掉它们；`remove_import` 不碰 `imported_audit`。
            sqlx::query("DELETE FROM imported_session WHERE owner_node_id = ?1 AND export_id = ?2")
                .bind(owner_node_id)
                .bind(export_id)
                .execute(&mut *tx)
                .await
                .db()?;
        }
        sqlx::query("DELETE FROM imported_import WHERE import_id = ?1")
            .bind(write.import.as_str())
            .execute(&mut *tx)
            .await
            .db()?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        tx.commit().await.db()?;
        Ok(())
    }
}

/// 集合相等（顺序无关）：关联表是集合语义，写集两侧只需集合一致。
fn same_set(left: &[acp_core::model::ExportId], right: &[acp_core::model::ExportId]) -> bool {
    left.len() == right.len() && left.iter().all(|item| right.contains(item))
}
