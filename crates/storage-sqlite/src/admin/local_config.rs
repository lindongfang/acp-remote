//! `LocalConfigStore` 的 SQLite 实现（Agent profile、workspace、Provider 引用、首次初始化标记）。
//!
//! 权威是 `docs/CORE_PORTS_AND_STORAGE.md` §11.6 与 `CONFIG_REFERENCE.md` 的「配置与管理状态的权威」。
//! 三条要点：
//!
//! - **至多一个默认 profile**：`owned_agent_profile` 上有 `is_default = 1` 的部分唯一索引；`put_profile`
//!   在一次调用里先把旧默认降级、再写新行，因此调用返回后恰好一个默认，不存在两个/零个的可见中间态。
//! - **引用只记字段名**：`owned_provider_ref` 只有字段名、keystore 引用与版本；写入时校验 profile 的
//!   每个绑定都指向**已登记**的 Provider 字段（存在性只能在持久层判定，§11.6 把它定在写入时）。
//! - **种子幂等**：初始化标记存在即视为已初始化（`meta.local_config_seeded_at`），`mark_seeded` 随后成为
//!   无写入的 no-op；首轮导入与标记同一事务提交，空种子也写标记（§11.8 第 4 条／spec 的
//!   「首次初始化种子的幂等」）。
//!
//! 种子标记放在 `meta`（库级事实表，§7.3）：`v2` 的两族表里没有承载「本地配置已初始化」的列，而它必须
//! 与种子 profile 同事务提交，`meta` 是唯一既有的键值表。

use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;

use acp_core::model::{
    AgentId, AgentProfile, ConflictKind, PortError, ProviderEnvBinding, ProviderRef,
    ProviderRefKind, SeedState, Timestamp, WorkspaceAlias, WorkspaceRecord,
};
use acp_core::ports::{
    LocalConfigStore, ProfileWrite, ProviderRefWrite, SeedWrite, WorkspaceWrite,
};

use crate::admin::{
    BindingJson, decode_strings, decode_value, encode_strings, encode_value, enforce_capacity_gate,
    insert_audit_rows,
};
use crate::error::StorageError;
use crate::migrate::read_meta_row;
use crate::session_store::{Db, SqliteStore, decode, int, text};

/// `owned_agent_profile` 的读列。
const PROFILE_COLUMNS: &str = "agent_id, display_name, command, args_json, env_allowlist_json, \
     provider_env_json, is_default, created_at, updated_at";

/// `owned_workspace` 的读列。
const WORKSPACE_COLUMNS: &str = "alias, display_name, canonical_path, created_at, updated_at";

/// `owned_provider_ref` 的读列。
const PROVIDER_COLUMNS: &str = "provider_id, kind, display_name, configured_fields_json, \
     keystore_ref, version, updated_at";

/// 种子标记键（`meta`）：存在即「本地配置已完成首次初始化」。
const META_LOCAL_CONFIG_SEEDED_AT: &str = "local_config_seeded_at";

// ---------------------------------------------------------------------------------------------
// 行 → 值对象
// ---------------------------------------------------------------------------------------------

fn profile_from_row(row: &SqliteRow) -> Result<AgentProfile, StorageError> {
    let env = decode_value::<Vec<BindingJson>>(
        &text(row, "provider_env_json")?,
        "owned_agent_profile.provider_env_json is not a JSON array of bindings",
    )?
    .into_iter()
    .map(|binding| {
        ProviderEnvBinding::try_new(&binding.provider_id, &binding.field, &binding.name)
            .map_err(StorageError::from)
    })
    .collect::<Result<Vec<_>, _>>()?;
    AgentProfile::try_new(
        decode(&text(row, "agent_id")?, "owned_agent_profile.agent_id")?,
        &text(row, "display_name")?,
        &text(row, "command")?,
        decode_strings(
            &text(row, "args_json")?,
            "owned_agent_profile.args_json is not a JSON array",
        )?,
        decode_strings(
            &text(row, "env_allowlist_json")?,
            "owned_agent_profile.env_allowlist_json is not a JSON array",
        )?,
        env,
        int(row, "is_default")? != 0,
        decode(&text(row, "created_at")?, "owned_agent_profile.created_at")?,
        decode(&text(row, "updated_at")?, "owned_agent_profile.updated_at")?,
    )
    .map_err(StorageError::from)
}

fn workspace_from_row(row: &SqliteRow) -> Result<WorkspaceRecord, StorageError> {
    WorkspaceRecord::try_new(
        decode(&text(row, "alias")?, "owned_workspace.alias")?,
        &text(row, "display_name")?,
        &text(row, "canonical_path")?,
        decode(&text(row, "created_at")?, "owned_workspace.created_at")?,
        decode(&text(row, "updated_at")?, "owned_workspace.updated_at")?,
    )
    .map_err(StorageError::from)
}

fn provider_ref_from_row(row: &SqliteRow) -> Result<ProviderRef, StorageError> {
    let version = u64::try_from(sqlx::Row::try_get::<i64, _>(row, "version").map_err(|_| {
        StorageError::ColumnValue {
            column: "owned_provider_ref.version",
            expected: "non-negative version",
        }
    })?)
    .map_err(|_| StorageError::ColumnValue {
        column: "owned_provider_ref.version",
        expected: "non-negative version",
    })?;
    ProviderRef::try_new(
        &text(row, "provider_id")?,
        decode::<ProviderRefKind>(&text(row, "kind")?, "owned_provider_ref.kind")?,
        &text(row, "display_name")?,
        decode_strings(
            &text(row, "configured_fields_json")?,
            "owned_provider_ref.configured_fields_json is not a JSON array",
        )?,
        &text(row, "keystore_ref")?,
        version,
        decode(&text(row, "updated_at")?, "owned_provider_ref.updated_at")?,
    )
    .map_err(StorageError::from)
}

fn bindings_json(profile: &AgentProfile) -> Result<String, StorageError> {
    let bindings: Vec<BindingJson> = profile
        .env()
        .iter()
        .map(|binding| BindingJson {
            provider_id: binding.provider_id().to_owned(),
            field: binding.field().to_owned(),
            name: binding.name().to_owned(),
        })
        .collect();
    encode_value(&bindings, "profile bindings are not encodable")
}

/// §11.6：profile 的每个绑定必须指向已登记的 Provider 字段（存在性只能在持久层判定）。
async fn ensure_bindings_are_registered(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    profile: &AgentProfile,
) -> Result<(), StorageError> {
    for binding in profile.env() {
        let fields: Option<String> = sqlx::query_scalar(
            "SELECT configured_fields_json FROM owned_provider_ref WHERE provider_id = ?1",
        )
        .bind(binding.provider_id())
        .fetch_optional(&mut **tx)
        .await
        .db()?;
        let Some(fields) = fields else {
            return Err(StorageError::InvalidRequest(
                "profile references an unregistered provider",
            ));
        };
        let registered = decode_strings(
            &fields,
            "owned_provider_ref.configured_fields_json is not a JSON array",
        )?;
        if !registered.iter().any(|field| field == binding.field()) {
            return Err(StorageError::InvalidRequest(
                "profile references an unconfigured provider field",
            ));
        }
    }
    Ok(())
}

async fn upsert_profile(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    profile: &AgentProfile,
) -> Result<(), StorageError> {
    ensure_bindings_are_registered(tx, profile).await?;
    if profile.is_default() {
        // 一次调用内的原子切换：先降级旧默认，再写新行（部分唯一索引保证不会出现两个默认）。
        sqlx::query("UPDATE owned_agent_profile SET is_default = 0 WHERE is_default = 1")
            .execute(&mut **tx)
            .await
            .db()?;
    }
    sqlx::query(
        "INSERT INTO owned_agent_profile (agent_id, display_name, command, args_json, \
         env_allowlist_json, provider_env_json, is_default, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
         ON CONFLICT(agent_id) DO UPDATE SET display_name = excluded.display_name, \
         command = excluded.command, args_json = excluded.args_json, \
         env_allowlist_json = excluded.env_allowlist_json, \
         provider_env_json = excluded.provider_env_json, is_default = excluded.is_default, \
         updated_at = excluded.updated_at",
    )
    .bind(profile.id().as_str())
    .bind(profile.display_name())
    .bind(profile.command())
    .bind(encode_strings(profile.args().iter().cloned()))
    .bind(encode_strings(profile.env_allowlist().iter().cloned()))
    .bind(bindings_json(profile)?)
    .bind(if profile.is_default() { "1" } else { "0" })
    .bind(profile.created_at().as_str())
    .bind(profile.updated_at().as_str())
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(StorageError::from)
}

fn seeded_at(row: Option<String>) -> Result<SeedState, StorageError> {
    match row {
        None => Ok(SeedState::unseeded()),
        Some(value) => Ok(SeedState::try_new(
            true,
            Some(decode::<Timestamp>(&value, META_LOCAL_CONFIG_SEEDED_AT)?),
        )?),
    }
}

#[async_trait]
impl LocalConfigStore for SqliteStore {
    async fn profiles(&self) -> Result<Vec<AgentProfile>, PortError> {
        let sql =
            format!("SELECT {PROFILE_COLUMNS} FROM owned_agent_profile ORDER BY agent_id ASC");
        let rows = sqlx::query(&sql).fetch_all(&self.pools().read).await.db()?;
        let mut profiles = Vec::with_capacity(rows.len());
        for row in &rows {
            profiles.push(profile_from_row(row)?);
        }
        Ok(profiles)
    }

    async fn profile(&self, id: &AgentId) -> Result<Option<AgentProfile>, PortError> {
        let sql = format!("SELECT {PROFILE_COLUMNS} FROM owned_agent_profile WHERE agent_id = ?1");
        let row = sqlx::query(&sql)
            .bind(id.as_str())
            .fetch_optional(&self.pools().read)
            .await
            .db()?;
        Ok(row.as_ref().map(profile_from_row).transpose()?)
    }

    /// §11.6：`put_profile` 是唯一写入默认 profile 的入口；切换默认是一次调用的原子写集。
    async fn put_profile(&self, write: ProfileWrite) -> Result<(), PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        upsert_profile(&mut tx, &write.profile).await?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    async fn workspaces(&self) -> Result<Vec<WorkspaceRecord>, PortError> {
        let sql = format!("SELECT {WORKSPACE_COLUMNS} FROM owned_workspace ORDER BY alias ASC");
        let rows = sqlx::query(&sql).fetch_all(&self.pools().read).await.db()?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            records.push(workspace_from_row(row)?);
        }
        Ok(records)
    }

    async fn workspace(
        &self,
        alias: &WorkspaceAlias,
    ) -> Result<Option<WorkspaceRecord>, PortError> {
        let sql = format!("SELECT {WORKSPACE_COLUMNS} FROM owned_workspace WHERE alias = ?1");
        let row = sqlx::query(&sql)
            .bind(alias.as_str())
            .fetch_optional(&self.pools().read)
            .await
            .db()?;
        Ok(row.as_ref().map(workspace_from_row).transpose()?)
    }

    async fn put_workspace(&self, write: WorkspaceWrite) -> Result<(), PortError> {
        self.writable()?;
        let record = write.record;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        sqlx::query(
            "INSERT INTO owned_workspace (alias, display_name, canonical_path, created_at, \
             updated_at) VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(alias) DO UPDATE SET display_name = excluded.display_name, \
             canonical_path = excluded.canonical_path, updated_at = excluded.updated_at",
        )
        .bind(record.alias().as_str())
        .bind(record.display_name())
        .bind(record.canonical_path())
        .bind(record.created_at().as_str())
        .bind(record.updated_at().as_str())
        .execute(&mut *tx)
        .await
        .db()
        .map_err(|error| error.into_conflict(ConflictKind::AlreadyExists))?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    async fn provider_refs(&self) -> Result<Vec<ProviderRef>, PortError> {
        let sql = format!(
            "SELECT {PROVIDER_COLUMNS} FROM owned_provider_ref ORDER BY provider_id ASC, kind ASC"
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pools().read).await.db()?;
        let mut refs = Vec::with_capacity(rows.len());
        for row in &rows {
            refs.push(provider_ref_from_row(row)?);
        }
        Ok(refs)
    }

    /// §11.2 第 7 条：换绑必须**递增**版本；不递增不被接受（旧引用继续有效）。
    async fn put_provider_ref(&self, write: ProviderRefWrite) -> Result<(), PortError> {
        self.writable()?;
        let reference = write.reference;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM owned_provider_ref WHERE provider_id = ?1 AND kind = ?2",
        )
        .bind(reference.id())
        .bind(reference.kind().as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?;
        if let Some(current) = current {
            let current = u64::try_from(current).map_err(|_| StorageError::ColumnValue {
                column: "owned_provider_ref.version",
                expected: "non-negative version",
            })?;
            if reference.version() <= current {
                return Err(PortError::Conflict(ConflictKind::VersionMismatch));
            }
        }
        let version = i64::try_from(reference.version()).map_err(|_| {
            StorageError::InvalidRequest("provider reference version is out of range")
        })?;
        sqlx::query(
            "INSERT INTO owned_provider_ref (provider_id, kind, display_name, \
             configured_fields_json, keystore_ref, version, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(provider_id, kind) DO UPDATE SET display_name = excluded.display_name, \
             configured_fields_json = excluded.configured_fields_json, \
             keystore_ref = excluded.keystore_ref, version = excluded.version, \
             updated_at = excluded.updated_at",
        )
        .bind(reference.id())
        .bind(reference.kind().as_str())
        .bind(reference.display_name())
        .bind(encode_strings(
            reference.configured_fields().iter().cloned(),
        ))
        .bind(reference.keystore_ref())
        .bind(version)
        .bind(reference.updated_at().as_str())
        .execute(&mut *tx)
        .await
        .db()
        .map_err(|error| error.into_conflict(ConflictKind::AlreadyExists))?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    async fn seed_state(&self) -> Result<SeedState, PortError> {
        Ok(seeded_at(
            read_meta_row(&self.pools().read, META_LOCAL_CONFIG_SEEDED_AT).await?,
        )?)
    }

    /// §11.8 第 4 条：种子 profile 与「已初始化」标记同一事务提交（空列表也写标记）；已初始化时
    /// **不产生任何写入**（种子被忽略，库内值保持权威，spec 的「重复打开不重导种子」）。
    async fn mark_seeded(&self, write: SeedWrite) -> Result<(), PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let already = read_meta_row(&mut *tx, META_LOCAL_CONFIG_SEEDED_AT).await?;
        if already.is_some() {
            // 已初始化：种子被忽略；这里不回滚也不写任何行。
            tx.commit().await.db()?;
            return Ok(());
        }
        for profile in &write.profiles {
            let existing: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM owned_agent_profile WHERE agent_id = ?1")
                    .bind(profile.id().as_str())
                    .fetch_optional(&mut *tx)
                    .await
                    .db()?;
            if existing.is_some() {
                // 「已有同 ID 管理记录与种子不一致时显式报错，不覆盖、不合并」。
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            upsert_profile(&mut tx, profile).await?;
        }
        sqlx::query("INSERT INTO meta (key, value) VALUES (?1, ?2)")
            .bind(META_LOCAL_CONFIG_SEEDED_AT)
            .bind(write.context.at.as_str())
            .execute(&mut *tx)
            .await
            .db()?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
        tx.commit().await.db()?;
        Ok(())
    }
}
