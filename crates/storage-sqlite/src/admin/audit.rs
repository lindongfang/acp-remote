//! `AuditStore` 的 SQLite 实现（`docs/CORE_PORTS_AND_STORAGE.md` §5.3 的端口形状、§7.3 的
//! `owned_audit` DDL、§11.6 第 6 条）。
//!
//! 四条落点决策：
//!
//! - **表选择**：`append` 只写本节点权威的 `owned_audit`。`imported_audit`（§7.4）保存的是**从 Owner
//!   收到的**审计元数据（带 `owner_node_id`/`export_id`/`session_id`/`request_id`），本地动作不写它；
//!   混在一张表里会让「这条审计是不是本机产生的」不可判定（`admin/mod.rs` 的模块文档）。`query`
//!   因此也只读 `owned_audit`：`AuditRecord` 没有承载来源节点/Export 的字段，把 imported 行投影成
//!   本类型会静默丢掉 §7.4 的归属列。
//! - **编码复用**：`append` 走 [`insert_audit_rows`]——与状态写集同一条 `INSERT`、同一套
//!   `actor_key`/`EntityRef::kind`/`target_id` 取值编码，不另造一套序列化，也不新增列（DDL 由合同
//!   漂移门禁钉死）。
//! - **不读系统时间**：`append` 的时间来自记录自身（调用方从 `Clock` 取得），容量门与保留窗口也用
//!   同一时刻（§2）。
//! - **失败关闭**：损坏的列值按既有惯例映射（`decode` → `ColumnValue` / `Corrupt`），不 panic、不静默
//!   取默认值。

use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;

use acp_core::model::{Actor, ActorKind, AuditRecord, EntityRef, PortError, ScopeSet};
use acp_core::ports::{AuditQuery, AuditStore, PendingAudit};

use crate::admin::{enforce_capacity_gate, insert_audit_rows};
use crate::error::StorageError;
use crate::session_store::{Db, SqliteStore, decode, decode_opt, opt_text, text};

/// `owned_audit` 的读列（与 §7.3 的 DDL 逐列一致；`audit_id` 只用于同刻排序，不进 `AuditRecord`）。
const AUDIT_COLUMNS: &str = "at, action, actor_kind, actor_id, via_node_id, local_principal_ref, \
     target_kind, target_id, outcome, detail_digest";

// ---------------------------------------------------------------------------------------------
// 值对象 ↔ 行
// ---------------------------------------------------------------------------------------------

/// `AuditRecord` → 写集的 [`PendingAudit`]：逐字段搬移（`at` 由 `insert_audit_rows` 单独绑定，
/// 这是 §11.6 的写集模型里唯一被抽出的列）。
fn pending_audit(record: &AuditRecord) -> PendingAudit {
    PendingAudit {
        action: record.action(),
        actor: record.actor().clone(),
        via_node: record.via_node().cloned(),
        local_principal_ref: record.local_principal_ref().map(str::to_owned),
        target: record.target().clone(),
        outcome: record.outcome(),
        detail_digest: record.detail_digest().cloned(),
    }
}

/// 从 §7.3 的 `(actor_kind, actor_id)` 还原 `Actor`。
///
/// 与 `owned_command` 的还原同款、**同样有损**：审计列只有这两列，设备 scopes 不在表里——审计只记录
/// 归因，授权不在持久层判定。Node 的复合键按 `"{node}/{access_node}"` 拆回（`Actor::id_text` 的形状）；
/// 认领方（`pairing_claimant`）的 `actor_id` 就是该配对 id。
fn actor_from_columns(kind: ActorKind, id: &str) -> Result<Actor, StorageError> {
    match kind {
        ActorKind::Device => Ok(Actor::Device {
            device: decode(id, "owned_audit.actor_id")?,
            scopes: ScopeSet::empty(),
        }),
        ActorKind::Node => {
            let (node, access_node) = id.split_once('/').ok_or(StorageError::ColumnValue {
                column: "owned_audit.actor_id",
                expected: "node/access-node pair",
            })?;
            Ok(Actor::Node {
                node: decode(node, "owned_audit.actor_id")?,
                access_node: decode(access_node, "owned_audit.actor_id")?,
            })
        }
        ActorKind::Cli => Ok(Actor::LocalCli),
        ActorKind::PairingClaimant => Ok(Actor::PairingClaimant {
            pairing: decode(id, "owned_audit.actor_id")?,
        }),
    }
}

/// 从 §7.3 的 `(target_kind, target_id)` 还原 `EntityRef`。
///
/// `target_kind` 列没有表级 CHECK（§7.3 只固定 `actor_kind`），因此未知类别按列值损坏处理，不猜一个
/// 近似实体。token 与 `EntityRef::kind()` 逐字相同；`command` 的 id 是 `"{session}/{request}"` 或
/// 分离式命令的 `"{request}"`（各 id 的字符集都不含 `/`，因此 `split_once` 无歧义）。
fn target_from_columns(kind: &str, id: &str) -> Result<EntityRef, StorageError> {
    let target = match kind {
        "session" => EntityRef::Session(decode(id, "owned_audit.target_id")?),
        "turn" => EntityRef::Turn(decode(id, "owned_audit.target_id")?),
        "interaction" => EntityRef::Interaction(decode(id, "owned_audit.target_id")?),
        "command" => match id.split_once('/') {
            Some((session, request)) => EntityRef::Command {
                session: Some(decode(session, "owned_audit.target_id")?),
                request: decode(request, "owned_audit.target_id")?,
            },
            None => EntityRef::Command {
                session: None,
                request: decode(id, "owned_audit.target_id")?,
            },
        },
        "pairing" => EntityRef::Pairing(decode(id, "owned_audit.target_id")?),
        "device" => EntityRef::Device(decode(id, "owned_audit.target_id")?),
        "node" => EntityRef::Node(decode(id, "owned_audit.target_id")?),
        "export" => EntityRef::Export(decode(id, "owned_audit.target_id")?),
        "import" => EntityRef::Import(decode(id, "owned_audit.target_id")?),
        "provider" => EntityRef::Provider(id.to_owned()),
        _ => {
            return Err(StorageError::ColumnValue {
                column: "owned_audit.target_kind",
                expected: "known entity kind",
            });
        }
    };
    Ok(target)
}

fn audit_from_row(row: &SqliteRow) -> Result<AuditRecord, StorageError> {
    let at = decode(&text(row, "at")?, "owned_audit.at")?;
    let action = decode(&text(row, "action")?, "owned_audit.action")?;
    let actor_kind = decode(&text(row, "actor_kind")?, "owned_audit.actor_kind")?;
    let via_node = decode_opt(opt_text(row, "via_node_id")?, "owned_audit.via_node_id")?;
    let target_kind = text(row, "target_kind")?;
    let target = target_from_columns(&target_kind, &text(row, "target_id")?)?;
    let outcome = decode(&text(row, "outcome")?, "owned_audit.outcome")?;
    let detail_digest = decode_opt(opt_text(row, "detail_digest")?, "owned_audit.detail_digest")?;
    AuditRecord::try_new(
        at,
        action,
        actor_from_columns(actor_kind, &text(row, "actor_id")?)?,
        via_node,
        opt_text(row, "local_principal_ref")?,
        target,
        outcome,
        detail_digest,
    )
    .map_err(StorageError::from)
}

// ---------------------------------------------------------------------------------------------
// 查询条件 → SQL
// ---------------------------------------------------------------------------------------------

/// 追加一个条件，并把取值按 `?N` 的序号入队（`?N` 与 `query` 里的绑定顺序一一对应）。
fn push_condition(
    conditions: &mut Vec<String>,
    binds: &mut Vec<String>,
    column: &str,
    operator: &str,
    value: String,
) {
    binds.push(value);
    conditions.push(format!("{column} {operator} ?{}", binds.len()));
}

#[async_trait]
impl AuditStore for SqliteStore {
    /// 没有关联状态变更的审计（§11.6 第 6 条）：本方法单独提交**一条** `owned_audit` 行，不构成任何
    /// 「状态 + 审计」写集，也不用于伪造跨端口原子性。需要与状态同事务的审计仍走各自写集的
    /// `WriteContext.audit`（[`insert_audit_rows`]）。
    ///
    /// 容量门与其他新增行的写路径一致（§7.5 ⑥）：审计排在清理顺序最后，超限时宁可拒绝新写入也不静默
    /// 丢证据，因此这里也让写集回滚并返回 `Unavailable(StorageFull)`。
    async fn append(&self, record: AuditRecord) -> Result<(), PortError> {
        self.writable()?;
        let row = pending_audit(&record);
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        insert_audit_rows(&mut tx, record.at(), std::slice::from_ref(&row)).await?;
        enforce_capacity_gate(self, &mut tx, record.at()).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    /// 按 [`AuditQuery`] 读回审计行：`since`/`until` **含端点**（时间是定宽 UTC 毫秒文本，§3.2 的
    /// 字典序即时间序），`actions` 为空表示不按动作过滤，`actor`/`target` 按 `(kind, id)` 两列等值匹配，
    /// `limit` 为 `None` 时不加 `LIMIT`（合同：`None` = 不限）。
    ///
    /// 输出按 `at` 升序；同一时刻按 `audit_id` 升序，保证顺序稳定可比。条件无匹配（含 `since > until`）
    /// 返回空 `Vec` 而不是错误——「没有这样的记录」不是调用方的参数错误。
    async fn query(&self, query: AuditQuery) -> Result<Vec<AuditRecord>, PortError> {
        let mut sql = format!("SELECT {AUDIT_COLUMNS} FROM owned_audit");
        let mut conditions: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(since) = &query.since {
            push_condition(
                &mut conditions,
                &mut binds,
                "at",
                ">=",
                since.as_str().to_owned(),
            );
        }
        if let Some(until) = &query.until {
            push_condition(
                &mut conditions,
                &mut binds,
                "at",
                "<=",
                until.as_str().to_owned(),
            );
        }
        if !query.actions.is_empty() {
            let mut placeholders = Vec::with_capacity(query.actions.len());
            for action in &query.actions {
                binds.push(action.as_str().to_owned());
                placeholders.push(format!("?{}", binds.len()));
            }
            conditions.push(format!("action IN ({})", placeholders.join(", ")));
        }
        if let Some(actor) = &query.actor {
            push_condition(
                &mut conditions,
                &mut binds,
                "actor_kind",
                "=",
                actor.kind().as_str().to_owned(),
            );
            push_condition(
                &mut conditions,
                &mut binds,
                "actor_id",
                "=",
                actor.id_text(),
            );
        }
        if let Some(target) = &query.target {
            push_condition(
                &mut conditions,
                &mut binds,
                "target_kind",
                "=",
                target.kind().to_owned(),
            );
            push_condition(
                &mut conditions,
                &mut binds,
                "target_id",
                "=",
                target.target_id(),
            );
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY at ASC, audit_id ASC");
        if query.limit.is_some() {
            sql.push_str(&format!(" LIMIT ?{}", binds.len() + 1));
        }

        let mut statement = sqlx::query(&sql);
        for value in &binds {
            statement = statement.bind(value);
        }
        if let Some(limit) = query.limit {
            statement = statement.bind(i64::from(limit));
        }
        let rows = statement.fetch_all(&self.pools().read).await.db()?;

        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            records.push(audit_from_row(row)?);
        }
        Ok(records)
    }

    /// 管理写集的变更水位（`NODE_LINK_PROTOCOL.md` §12.3 的 `catalogRevision`）。
    ///
    /// 取 `sqlite_sequence` 里 `owned_audit` 的自增值（即 `audit_id` **曾经**写入过的最大值），不是
    /// `MAX(audit_id)`：后者在保留期清理把审计行全部删掉后会回退，而 `catalogRevision` 必须跨重启与
    /// 清理单调。`sqlite_sequence` 的那一行由 SQLite 维护、migration 重建审计表时刻意回填
    ///（`migrate.rs` 的 `audit_sequences`/`restore_audit_sequences`），因此它不是实现细节而是本水位的
    /// 权威来源。表/行缺失（未写过任何审计）时取 0。
    async fn watermark(&self) -> Result<u64, PortError> {
        let value: Option<i64> =
            sqlx::query_scalar("SELECT seq FROM sqlite_sequence WHERE name = 'owned_audit'")
                .fetch_optional(&self.pools().read)
                .await
                .db()?;
        match value {
            Some(value) => u64::try_from(value)
                .map_err(|_| PortError::Corrupt("owned_audit sqlite_sequence is negative")),
            None => Ok(0),
        }
    }
}
