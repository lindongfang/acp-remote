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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use acp_core::model::{
        Actor, AuditAction, AuditOutcome, DeviceId, EntityRef, NodeId, Timestamp,
    };

    use super::*;

    fn timestamp(text: &str) -> Timestamp {
        Timestamp::new(text).expect("timestamp")
    }

    fn record(at: &str, action: AuditAction, target: EntityRef) -> AuditRecord {
        AuditRecord::try_new(
            timestamp(at),
            action,
            Actor::LocalCli,
            None,
            None,
            target,
            AuditOutcome::Success,
            None,
        )
        .expect("audit record")
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "acpr-wp3b1-audit-{}-{sequence}-{name}",
            std::process::id()
        ))
    }

    fn records() -> Vec<AuditRecord> {
        vec![
            record(
                "2026-09-18T09:12:03.412Z",
                AuditAction::ExportRevoked,
                EntityRef::Export(acp_core::model::ExportId::new("export-2").expect("id")),
            ),
            record(
                "2026-09-18T08:00:00.000Z",
                AuditAction::DeviceRevoked,
                EntityRef::Device(
                    DeviceId::new("2ae1c07c-0000-4000-8000-000000000001").expect("id"),
                ),
            ),
        ]
    }

    #[test]
    fn jsonl_is_ascending_one_object_per_line() {
        let path = temp_path("export.jsonl");
        let (count, digest) =
            write_export_file(&path, AuditFormat::Jsonl, &records()).expect("写出");
        assert_eq!(count, 2);
        let text = fs::read_to_string(&path).expect("可读回");
        fs::remove_file(&path).ok();

        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let first: Value = serde_json::from_str(lines[0]).expect("合法 JSON");
        let second: Value = serde_json::from_str(lines[1]).expect("合法 JSON");
        assert_eq!(first["at"], Value::from("2026-09-18T08:00:00.000Z"));
        assert_eq!(second["at"], Value::from("2026-09-18T09:12:03.412Z"));
        assert_eq!(first["action"], Value::from("device.revoked"));
        assert_eq!(first["actorKind"], Value::from("cli"));
        assert_eq!(first["actorId"], Value::from("cli"));
        assert_eq!(first["targetKind"], Value::from("device"));
        assert_eq!(first["outcome"], Value::from("success"));
        assert_eq!(first["detailDigest"], Value::Null);
        assert_eq!(first["viaNodeId"], Value::Null);
        assert_eq!(first["localPrincipalRef"], Value::Null);
        assert_eq!(
            first.as_object().expect("object").len(),
            COLUMNS.len(),
            "导出对象只含 §14.2 的元数据列"
        );

        // 摘要与文件内容一致（§5.6 的 sha256）。
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let mut expected = String::new();
        for byte in hasher.finalize() {
            expected.push_str(&format!("{byte:02x}"));
        }
        assert_eq!(digest, expected);
        assert_eq!(digest.len(), 64);
        assert_eq!(digest, digest.to_lowercase());
    }

    #[test]
    fn csv_starts_with_the_column_header_and_escapes_fields() {
        let path = temp_path("export.csv");
        let mut with_principal = records();
        with_principal[1] = AuditRecord::try_new(
            timestamp("2026-09-18T08:00:00.000Z"),
            AuditAction::AuthorizationDenied,
            Actor::Node {
                node: NodeId::new("2ae1c07c-0000-4000-8000-000000000002").expect("node id"),
                access_node: NodeId::new("2ae1c07c-0000-4000-8000-000000000003").expect("node id"),
            },
            Some(NodeId::new("2ae1c07c-0000-4000-8000-000000000002").expect("node id")),
            Some("local,principal\"quoted".to_owned()),
            EntityRef::Provider("openai.primary".to_owned()),
            AuditOutcome::Denied,
            None,
        )
        .expect("audit record");
        let (count, _) = write_export_file(&path, AuditFormat::Csv, &with_principal).expect("写出");
        assert_eq!(count, 2);
        let text = fs::read_to_string(&path).expect("可读回");
        fs::remove_file(&path).ok();

        let mut lines = text.lines();
        assert_eq!(lines.next().expect("表头"), csv_header());
        let denied = lines.next().expect("第一行");
        assert!(denied.contains("\"authorization.denied\""));
        assert!(denied.contains("\"node\""));
        assert!(
            denied.contains("\"local,principal\"\"quoted\""),
            "逗号与引号必须被转义：{denied}"
        );
        assert!(denied.contains("\"openai.primary\""));
        assert!(lines.next().is_some(), "两行数据");
        assert!(lines.next().is_none());
        // actorId 承载 Node Link 的复合幂等键（`{node}/{access_node}`）。
        assert!(denied.contains(
            "\"2ae1c07c-0000-4000-8000-000000000002/2ae1c07c-0000-4000-8000-000000000003\""
        ));
    }

    #[test]
    fn an_existing_output_file_is_never_overwritten() {
        let path = temp_path("existing.jsonl");
        fs::write(&path, b"keep me").expect("预置文件");
        let error = write_export_file(&path, AuditFormat::Jsonl, &records()).expect_err("必须冲突");
        assert_eq!(error.code(), LocalErrorCode::Conflict);
        assert_eq!(fs::read_to_string(&path).expect("可读回"), "keep me");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn an_empty_selection_writes_an_empty_document() {
        let path = temp_path("empty.jsonl");
        let (count, digest) = write_export_file(&path, AuditFormat::Jsonl, &[]).expect("写出");
        assert_eq!(count, 0);
        assert_eq!(fs::read(&path).expect("可读回"), Vec::<u8>::new());
        // 空内容的 SHA-256（可复算，证明摘要取自实际写出的字节）。
        assert_eq!(
            digest,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        fs::remove_file(&path).ok();

        let csv = temp_path("empty.csv");
        write_export_file(&csv, AuditFormat::Csv, &[]).expect("写出");
        assert_eq!(
            fs::read_to_string(&csv).expect("可读回"),
            format!("{}\n", csv_header())
        );
        fs::remove_file(&csv).ok();
    }
}
