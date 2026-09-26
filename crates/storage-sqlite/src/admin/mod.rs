//! 管理状态（设备/节点/配对/身份材料、Export/Import、本地配置）的 SQLite 实现。
//!
//! 权威是 `docs/CORE_PORTS_AND_STORAGE.md`：§5.3 的端口形状、§7.3/§7.4 的表结构、§9 判据 23–29 与
//! §11.2/§11.6 的写集语义。三条硬规则贯穿本目录：
//!
//! - **一次端口调用 = 一个事务 = 一个完整写集**：状态、集合字段与审计在同一事务里提交；任一约束失败
//!   整事务回滚，绝不「先提交状态再补审计」（§11.2 第 6 条、§9 判据 23）。
//! - **失败关闭**：约束失败映射成语义化的 `PortError::Conflict`（不落进 `Backend`），损坏或只读时写
//!   路径一律 `PortError::Corrupt`（§8、§9 判据 29）。
//! - **不读系统时间**：所有时间来自 `WriteContext.at`（§2）；本目录不调用任何时钟。
//!
//! 集合字段的编解码是本适配器的职责（§11.1：`*_json` 在 adapter 内按领域构造器校验后编码）：所有
//! `*_json` 列都是「有类型的 JSON 数组文本」，空集合固定写 `[]`；读取一律回到 `core::model` 的构造器
//! 重新校验，格式非法按**损坏**处理（`Corrupt`），不静默降级。
//!
//! 审计落点：管理写集的审计行写进 `owned_audit`。`imported_audit`（§7.4）保存的是**从 Owner 收到的**
//! 审计元数据（带 `owner_node_id`/`export_id`），本地管理动作不写它——两者混在一张表里会让「这条审计
//! 是不是本机产生的」不可判定。

pub(crate) mod audit;
pub(crate) mod export;
pub(crate) mod local_config;
pub(crate) mod trust;

use acp_core::model::{Digest, NodeId, PortError, Timestamp};
use acp_core::ports::PendingAudit;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Sqlite, Transaction};

use crate::error::StorageError;
use crate::session_store::{SqliteStore, actor_key};

// ---------------------------------------------------------------------------------------------
// 审计（§7.3 的 `owned_audit`）
// ---------------------------------------------------------------------------------------------

/// 写集里的一条 `PendingAudit` 与状态同事务落库（§11.6 的提交模型）。
const INSERT_AUDIT: &str = "INSERT INTO owned_audit (at, action, actor_kind, actor_id, via_node_id, \
     local_principal_ref, target_kind, target_id, outcome, detail_digest) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

/// 写入写集携带的全部审计行。任一失败（唯一键、CHECK、磁盘满）都会让整个事务回滚。
pub(crate) async fn insert_audit_rows(
    tx: &mut Transaction<'_, Sqlite>,
    at: &Timestamp,
    audit: &[PendingAudit],
) -> Result<(), StorageError> {
    for row in audit {
        let (kind, id) = actor_key(&row.actor);
        sqlx::query(INSERT_AUDIT)
            .bind(at.as_str())
            .bind(row.action.as_str())
            .bind(kind.as_str())
            .bind(&id)
            .bind(row.via_node.as_ref().map(NodeId::as_str))
            .bind(row.local_principal_ref.as_deref())
            .bind(row.target.kind())
            .bind(row.target.target_id())
            .bind(row.outcome.as_str())
            .bind(row.detail_digest.as_ref().map(Digest::as_str))
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// 为过期扫描补写 `pairing.expired`（§11.6 第 6 条）：每条被终结的配对一行，目标即该配对。
///
/// 归因取 `ExpiryWrite.context.audit` 的首条（`UseCases::expire_pairings` 的文档把 actor 定在那里）；
/// 调用方传空集合时（当前实现如此）退回 `LocalCli`——周期清理是 daemon 自己的动作，而 §7.3 的
/// `actor_kind` 词表里 `cli` 正是本机入口。
pub(crate) async fn insert_expiry_audit(
    tx: &mut Transaction<'_, Sqlite>,
    at: &Timestamp,
    audit: &[PendingAudit],
    pairing: &acp_core::model::PairingId,
) -> Result<(), StorageError> {
    let template = audit.first();
    let actor = template.map_or(acp_core::model::Actor::LocalCli, |row| row.actor.clone());
    let (kind, id) = actor_key(&actor);
    sqlx::query(INSERT_AUDIT)
        .bind(at.as_str())
        .bind(acp_core::model::AuditAction::PairingExpired.as_str())
        .bind(kind.as_str())
        .bind(&id)
        .bind(
            template
                .and_then(|row| row.via_node.as_ref())
                .map(NodeId::as_str),
        )
        .bind(template.and_then(|row| row.local_principal_ref.as_deref()))
        .bind("pairing")
        .bind(pairing.as_str())
        .bind(acp_core::model::AuditOutcome::Success.as_str())
        .bind(None::<&str>)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// §7.5 ⑥：管理写集与 owned/imported 路径共用的容量门。
///
/// 调用位置与 `SessionStore::commit` 一致——变更与审计写完之后、提交之前：度量因此包含本次写集。
/// 超限时按 ①→②→③ 清理已过窗口的正文（**永不**删除活动信任、撤销记录或未到期审计），仍超限则
/// 返回 `Unavailable(StorageFull)`，由调用方回滚整个写集（不留下半条授权）。
///
/// 只加在**会新增行**的写路径上（`put_*`/`create_pairing`/`claim_pairing`/`settle_pairing`/
/// `add_import`/`mark_seeded`）。撤销、移除、过期扫描与**配对消费**（`consume_pairing`，只推进状态、
/// 不新增管理行）既不增长库、又是安全动作，必须保持可用（§11.2 第 4/5 条要求撤销先提交再阻断访问）。
pub(crate) async fn enforce_capacity_gate(
    store: &SqliteStore,
    tx: &mut Transaction<'_, Sqlite>,
    at: &Timestamp,
) -> Result<(), PortError> {
    crate::session_store::enforce_capacity(tx, &store.window(), at, None, store.measure_sql()).await
}

// ---------------------------------------------------------------------------------------------
// 集合字段（§11.1 的 `*_json`）
// ---------------------------------------------------------------------------------------------

/// 把一组文本编码成 JSON 数组文本（空集合自然得到 `[]`）。
pub(crate) fn encode_strings(values: impl IntoIterator<Item = String>) -> String {
    Value::Array(values.into_iter().map(Value::String).collect()).to_string()
}

/// 解析 `*_json` 的字符串数组。文本非法说明库被外部改写 → 损坏（不静默取空集合）。
pub(crate) fn decode_strings(
    text: &str,
    column: &'static str,
) -> Result<Vec<String>, StorageError> {
    serde_json::from_str(text).map_err(|_| StorageError::Corrupt(column))
}

/// workspace alias 声明（`aliases_json`，§11.7 的 `[{alias,displayName}]`）。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AliasJson {
    pub alias: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

/// Export template 的参数声明（`templates_json` 的嵌套元素）。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParamJson {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<Value>>,
}

/// Export template（`templates_json`）。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TemplateJson {
    pub template_id: String,
    pub display_name: String,
    pub workspace_alias: String,
    pub params: Vec<ParamJson>,
}

/// profile 的凭据绑定（`provider_env_json`）。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BindingJson {
    pub provider_id: String,
    pub field: String,
    pub name: String,
}

/// 编码任意集合字段：序列化失败只可能是形状错误（本适配器自己构造的 DTO），因此映射成 `InvalidRequest`
/// 而不是 panic。
pub(crate) fn encode_value<T: Serialize>(
    value: &T,
    what: &'static str,
) -> Result<String, StorageError> {
    serde_json::to_value(value)
        .map(|value| value.to_string())
        .map_err(|_| StorageError::InvalidRequest(what))
}

/// 解码集合字段；形状不符按损坏处理。
pub(crate) fn decode_value<T: for<'de> Deserialize<'de>>(
    text: &str,
    column: &'static str,
) -> Result<T, StorageError> {
    serde_json::from_str(text).map_err(|_| StorageError::Corrupt(column))
}
