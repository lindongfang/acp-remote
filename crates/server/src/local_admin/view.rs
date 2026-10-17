//! core 值对象 → 管理 `result` 的投影（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.2–§5.5）。
//!
//! 只做形状投影：字段名一律 `camelCase`（§1.1），时间戳是 §1.1 的 RFC 3339 毫秒文本，数组保序。
//! 这里**不**做校验（校验在 [`crate::local_admin::params`]）、不读时钟、不接触存储；导出给
//! `result` 的字段集合与 §5.2–§5.5 的表格逐项对应，未知字段不会被凭空造出来。

use acp_core::model::{DeviceRecord, NodeRecord, Timestamp};
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

/// `device.list` 的 `DeviceRecord`（§5.3）：含 `pending` 与 `revoked` 记录，**不含授权集合**
/// （设备权限层已删除：配对不授予授权，设备记录也不保存授权集合）。
pub(crate) fn device(record: &DeviceRecord) -> JsonObject {
    object(vec![
        ("deviceId", text(record.device_id().as_str())),
        (
            "publicKeyFingerprint",
            text(record.public_key_fingerprint().as_str()),
        ),
        ("state", text(record.state().as_str())),
        ("createdAt", timestamp(record.created_at())),
        ("lastSeenAt", optional_timestamp(record.last_seen_at())),
        ("revokedAt", optional_timestamp(record.revoked_at())),
    ])
}

/// `node.list` 的 `NodeRecord`（§5.4）：`ownerEndpoint` 只在 `kind = "owner"` 时非 `null`。
pub(crate) fn node(record: &NodeRecord) -> JsonObject {
    object(vec![
        ("nodeId", text(record.node_id().as_str())),
        ("kind", text(record.kind().as_str())),
        (
            "nodePublicKeyFingerprint",
            text(record.node_public_key_fingerprint().as_str()),
        ),
        ("state", text(record.state().as_str())),
        (
            "ownerEndpoint",
            match record.owner_endpoint() {
                Some(endpoint) => text(endpoint),
                None => Value::Null,
            },
        ),
        ("createdAt", timestamp(record.created_at())),
        (
            "lastConnectedAt",
            optional_timestamp(record.last_connected_at()),
        ),
        ("revokedAt", optional_timestamp(record.revoked_at())),
    ])
}

/// 可空时间戳（§1.1：`T | null` 字段用 `null` 而不是缺字段）。
pub(crate) fn optional_timestamp(value: Option<&Timestamp>) -> Value {
    match value {
        Some(at) => timestamp(at),
        None => Value::Null,
    }
}
