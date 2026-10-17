//! `audit.export` 的写出阶段（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.6、
//! `docs/SECURITY_DESIGN.md` §14.1/§14.2）。
//!
//! 职责边界：
//!
//! - 取记录由路由经注入的 `Arc<dyn AuditStore>` 完成（§5.1：审计导出「不经业务用例」）；
//! - 本模块只做**纯序列化 + 写文件 + 摘要**：按记录时间升序、`jsonl` 每行一个对象、`csv` 首行列名；
//!   列集合固定为 §14.2 的元数据字段，**不可能**包含 prompt、回复、diff、终端、ACP `rawJson`、附件、
//!   凭据、pairing secret 或 QR payload——那些内容既不在 [`AuditRecord`] 里，也不在本模块的列清单里。
//! - 输出文件以 `create_new` 方式创建：已存在即 `local.conflict`（§5.6），绝不覆盖已有文件。

use std::fs::OpenOptions;
use std::io::Write;

use acp_core::model::AuditRecord;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use crate::local_admin::envelope::JsonObject;
use crate::local_admin::error::{AdminError, LocalErrorCode};
use crate::local_admin::params::AuditFormat;
use crate::local_admin::view::object;

/// 审计导出表的列（顺序即 `csv` 的列顺序，也是 `jsonl` 的字段集合）。
const COLUMNS: [&str; 10] = [
    "at",
    "action",
    "actorKind",
    "actorId",
    "viaNodeId",
    "localPrincipalRef",
    "targetKind",
    "targetId",
    "outcome",
    "detailDigest",
];

/// 一条审计记录的元数据对象（§14.2）：列名与 `docs/CORE_PORTS_AND_STORAGE.md` §7.3 的 `owned_audit`
/// 列一一对应，值只来自 [`AuditRecord`] 的只读访问器。
pub(crate) fn record_json(record: &AuditRecord) -> JsonObject {
    let target = record.target();
    object(vec![
        ("at", Value::String(record.at().as_str().to_owned())),
        ("action", Value::String(record.action().as_str().to_owned())),
        (
            "actorKind",
            Value::String(record.actor().kind().as_str().to_owned()),
        ),
        ("actorId", Value::String(record.actor().id_text())),
        (
            "viaNodeId",
            match record.via_node() {
                Some(node) => Value::String(node.as_str().to_owned()),
                None => Value::Null,
            },
        ),
        (
            "localPrincipalRef",
            match record.local_principal_ref() {
                Some(reference) => Value::String(reference.to_owned()),
                None => Value::Null,
            },
        ),
        ("targetKind", Value::String(target.kind().to_owned())),
        ("targetId", Value::String(target.target_id())),
        (
            "outcome",
            Value::String(record.outcome().as_str().to_owned()),
        ),
        (
            "detailDigest",
            match record.detail_digest() {
                Some(digest) => Value::String(digest.as_str().to_owned()),
                None => Value::Null,
            },
        ),
    ])
}

/// 按记录时间**升序**写出审计导出文件，返回 `(recordCount, sha256)`。
///
/// 时间戳是定宽 UTC 文本（§1.1），字典序即时间序；排序稳定，因此同一时刻的多条记录保持存储给出的相对顺序。
pub(crate) fn write_export_file(
    path: &std::path::Path,
    format: AuditFormat,
    records: &[AuditRecord],
) -> Result<(u64, String), AdminError> {
    let mut ordered: Vec<&AuditRecord> = records.iter().collect();
    ordered.sort_by(|left, right| left.at().as_str().cmp(right.at().as_str()));

    let bytes = match format {
        AuditFormat::Jsonl => jsonl_bytes(&ordered)?,
        AuditFormat::Csv => csv_bytes(&ordered),
    };

    // 已存在即冲突（§5.6）：先判定一次给出可判定的错误，再用 `create_new` 兜住并发窗口。
    if path.exists() {
        return Err(AdminError::new(
            LocalErrorCode::Conflict,
            "audit.export: output file already exists",
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => AdminError::new(
                LocalErrorCode::Conflict,
                "audit.export: output file already exists",
            ),
            _ => AdminError::new(
                LocalErrorCode::Internal,
                "audit.export: output file cannot be created",
            ),
        })?;
    file.write_all(&bytes).map_err(|_| {
        AdminError::new(
            LocalErrorCode::Internal,
            "audit.export: output file cannot be written",
        )
    })?;
    file.flush().map_err(|_| {
        AdminError::new(
            LocalErrorCode::Internal,
            "audit.export: output file cannot be flushed",
        )
    })?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let mut digest = String::with_capacity(64);
    for byte in hasher.finalize() {
        digest.push_str(&format!("{byte:02x}"));
    }
    Ok((ordered.len() as u64, digest))
}

fn jsonl_bytes(records: &[&AuditRecord]) -> Result<Vec<u8>, AdminError> {
    let mut buffer = Vec::new();
    for record in records {
        let line = serde_json::to_string(&Value::Object(record_json(record))).map_err(|_| {
            AdminError::new(
                LocalErrorCode::Internal,
                "audit.export: audit record cannot be serialized",
            )
        })?;
        buffer.extend_from_slice(line.as_bytes());
        buffer.push(b'\n');
    }
    Ok(buffer)
}

fn csv_bytes(records: &[&AuditRecord]) -> Vec<u8> {
    let mut buffer = String::new();
    buffer.push_str(&csv_header());
    buffer.push('\n');
    for record in records {
        let json = record_json(record);
        let row: Vec<String> = COLUMNS
            .iter()
            .map(|column| csv_field(json.get(*column)))
            .collect();
        buffer.push_str(&row.join(","));
        buffer.push('\n');
    }
    buffer.into_bytes()
}

/// `csv` 的首行：列名（与 [`COLUMNS`] 同序，按 RFC 4180 最小实现一律加引号）。
fn csv_header() -> String {
    COLUMNS
        .iter()
        .map(|column| format!("\"{column}\""))
        .collect::<Vec<_>>()
        .join(",")
}

/// RFC 4180 的最小实现：每个字段一律加引号，内部引号成对。
fn csv_field(value: Option<&Value>) -> String {
    let text = match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    format!("\"{}\"", text.replace('"', "\"\""))
}
