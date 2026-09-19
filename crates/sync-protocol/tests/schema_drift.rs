//! 合同漂移门禁：Rust 里的消息类型、视图封闭 enum 与错误码必须与 schema / registry 逐条相等。
//!
//! 两侧都是机器资产（`schemas/sync/v1/*.schema.json`、`compatibility/errors/v1/errors.json`），
//! 任一侧单独改动都会让这里失败——这就是"实现不会静默落后于合同"的判据。
//!
//! 消息类型的抽取规则与 `scripts/check-schema-fixtures.mjs` 的 `declaredTypes` 一致：从
//! `message.schema.json` 的 union 出发，只沿 `$ref` 与 `allOf`/`oneOf`/`anyOf` 递归，**不进入**
//! `properties.*`。因此 `common.schema.json` 里 content block 的判别子（`text`/`image_ref`/…）
//! 不会被误收——这正是"按 glob 全文件抽 const"会得到 22 个而非 18 个的原因。

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use sync_protocol::envelope::MessageType;
use sync_protocol::error::ErrorCode;
use sync_protocol::views::{self, VIEW_TYPES};

/// `schemas/sync/v1/` 下的全部 schema，按文件名索引。
struct SchemaSet {
    documents: BTreeMap<String, serde_json::Value>,
}

impl SchemaSet {
    fn load(root: &Path) -> Self {
        let mut documents = BTreeMap::new();
        for entry in std::fs::read_dir(root).expect("schemas/sync/v1 目录") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("schema 文件名")
                .to_owned();
            documents.insert(name, support::read_json(&path));
        }
        Self { documents }
    }

    fn document(&self, name: &str) -> &serde_json::Value {
        self.documents
            .get(name)
            .unwrap_or_else(|| panic!("schema 文件不存在：{name}"))
    }

    /// 解析 `$ref`（含同文件 `#/$defs/x` 与跨文件 `common.schema.json#/$defs/x`）。
    fn resolve(&self, reference: &str, from: &str) -> (String, serde_json::Value) {
        let (file, pointer) = match reference.split_once('#') {
            Some((file, pointer)) => (if file.is_empty() { from } else { file }, pointer),
            None => (reference, ""),
        };
        let document = self.document(file);
        let node = if pointer.is_empty() || pointer == "/" {
            document.clone()
        } else {
            lookup_pointer(document, pointer)
                .unwrap_or_else(|| panic!("{file} 里没有指针 {pointer}"))
                .clone()
        };
        (file.to_owned(), node)
    }
}

fn lookup_pointer<'a>(
    document: &'a serde_json::Value,
    pointer: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = document;
    for segment in pointer.trim_start_matches('/').split('/') {
        let decoded = segment.replace("~1", "/").replace("~0", "~");
        current = current.get(&decoded)?;
    }
    Some(current)
}

/// 递归收集子树里每个 `enum` 关键字的 JSON Pointer（相对子树根，RFC 6901 转义）。
///
/// 不跟随 `$ref`：`event-views.schema.json` 里被引用的 `common.schema.json` 封闭 enum 属于
/// 另一张表（消息/公共类型的镜像门禁）的职责。
fn collect_enum_pointers(node: &serde_json::Value, pointer: &str, found: &mut Vec<String>) {
    match node {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if key == "enum" {
                    found.push(pointer.to_owned());
                    continue;
                }
                let child = format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                collect_enum_pointers(value, &child, found);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_enum_pointers(item, &format!("{pointer}/{index}"), found);
            }
        }
        _ => {}
    }
}

/// 与 JS 侧同源的抽取：只沿 union 与 `$ref` 走，不进入 `properties`。
fn collect_declared_types(
    set: &SchemaSet,
    file: &str,
    node: &serde_json::Value,
    found: &mut BTreeSet<String>,
    depth: usize,
) {
    if depth > 12 {
        return;
    }
    match node {
        serde_json::Value::Object(map) => {
            if let Some(reference) = map.get("$ref").and_then(serde_json::Value::as_str) {
                let (target_file, target_node) = set.resolve(reference, file);
                collect_declared_types(set, &target_file, &target_node, found, depth + 1);
            }
            if let Some(constant) = map
                .get("properties")
                .and_then(|properties| properties.get("type"))
                .and_then(|type_schema| type_schema.get("const"))
                .and_then(serde_json::Value::as_str)
            {
                found.insert(constant.to_owned());
            }
            for keyword in ["allOf", "oneOf", "anyOf"] {
                if let Some(branches) = map.get(keyword).and_then(serde_json::Value::as_array) {
                    for branch in branches {
                        collect_declared_types(set, file, branch, found, depth + 1);
                    }
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_declared_types(set, file, item, found, depth + 1);
            }
        }
        _ => {}
    }
}

fn schema_declared_types() -> BTreeSet<String> {
    let set = SchemaSet::load(&support::repo_path("schemas/sync/v1"));
    let message = set.document("message.schema.json").clone();
    let mut found = BTreeSet::new();
    collect_declared_types(&set, "message.schema.json", &message, &mut found, 0);
    found
}

#[test]
fn message_types_match_schema_union() {
    let declared: BTreeSet<String> = MessageType::ALL
        .iter()
        .map(|message_type| message_type.as_str().to_owned())
        .collect();
    let expected = schema_declared_types();

    assert_eq!(
        expected.len(),
        18,
        "从 message.schema.json 的 union 应当恰好抽出 18 个 type 常量，实际：{expected:?}"
    );
    let missing: Vec<&String> = expected.difference(&declared).collect();
    let extra: Vec<&String> = declared.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "schema 有而 Rust 没有：{missing:?}；Rust 有而 schema 没有：{extra:?}"
    );
    assert_eq!(declared.len(), 18, "v1 的 type 常量数应为 18");
    assert!(
        expected.iter().all(
            |constant| !["text", "image_ref", "resource_ref", "unsupported"]
                .contains(&constant.as_str())
        ),
        "content block 判别子不得进入消息类型集合"
    );

    for message_type in MessageType::ALL {
        let wire = serde_json::to_string(&message_type).expect("可序列化");
        assert_eq!(wire, format!("\"{}\"", message_type.as_str()));
        assert_eq!(
            message_type
                .as_str()
                .parse::<MessageType>()
                .expect("可解析"),
            message_type
        );
    }
}

#[test]
fn view_types_match_schema_defs() {
    let set = SchemaSet::load(&support::repo_path("schemas/sync/v1"));
    let defs = set.document("event-views.schema.json")["$defs"]
        .as_object()
        .expect("event-views 的 $defs");

    let declared: Vec<&str> = defs.keys().map(String::as_str).collect();
    assert_eq!(
        declared.len(),
        34,
        "event-views.schema.json 的 $defs 条目数应为 34，实际：{declared:?}"
    );
    assert_eq!(
        VIEW_TYPES.as_slice(),
        declared.as_slice(),
        "views::VIEW_TYPES 必须与 event-views.schema.json 的 $defs 逐条相等（两侧都按 serde_json 的 Map 键序，即字母序）"
    );

    for manifest in [
        "fixtures/sync/v1/manifest.json",
        "fixtures/node-link/v1/manifest.json",
    ] {
        for case in support::manifest_cases(manifest) {
            let Some(view_def) = case.view_def else {
                continue;
            };
            let event_type = view_def
                .strip_prefix("#/$defs/")
                .unwrap_or_else(|| panic!("{}：viewDef 形状不符：{view_def}", case.fixture));
            assert!(
                VIEW_TYPES.contains(&event_type),
                "{manifest} 的 {}：viewDef {event_type} 未登记在 VIEW_TYPES——Node Link 的事件视图契约与 Sync 共享",
                case.fixture
            );
        }
    }
}

/// 视图封闭 enum 的镜像门禁：`views::VIEW_ENUMS` 与 `event-views.schema.json` 双向相等。
///
/// 根因是"schema 里的封闭 enum 与 Rust 镜像之间没有判据"——`elicitation.resolved.action` 曾因此
/// 在 schema 增了 `decline` 之后仍只有两个变体。这里把两个方向都钉死：已登记条目必须等于 schema
/// 的 `enum`（逐条且同序），schema 里出现的每个 `enum` 也必须被登记（新增封闭 enum 时不能只改
/// schema 而不给镜像）。
#[test]
fn view_enums_match_schema() {
    let set = SchemaSet::load(&support::repo_path("schemas/sync/v1"));
    let document = set.document("event-views.schema.json").clone();
    let defs = document["$defs"].as_object().expect("event-views 的 $defs");

    // 登记条目 → schema：取值序列逐条相等，顺序也相等。
    for (event_type, pointer, values) in views::VIEW_ENUMS {
        let node = lookup_pointer(&document["$defs"][event_type], pointer)
            .unwrap_or_else(|| panic!("{event_type} 的 $defs 没有指针 {pointer}"));
        let declared: Vec<&str> = node["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("{event_type}{pointer} 不是 closed enum：{node}"))
            .iter()
            .map(|value| value.as_str().expect("enum 取值必须是字符串"))
            .collect();
        assert_eq!(
            declared, *values,
            "{event_type}{pointer} 的 schema enum 与 views::VIEW_ENUMS 不一致"
        );
    }

    // schema → 登记条目：不能有未登记的封闭 enum。
    let mut unregistered = Vec::new();
    for (event_type, definition) in defs {
        let mut found = Vec::new();
        collect_enum_pointers(definition, "", &mut found);
        for pointer in found {
            let registered =
                views::VIEW_ENUMS
                    .iter()
                    .any(|(registered_type, registered_pointer, _)| {
                        registered_type == event_type && *registered_pointer == pointer
                    });
            if !registered {
                unregistered.push(format!("{event_type}{pointer}"));
            }
        }
    }
    assert!(
        unregistered.is_empty(),
        "event-views.schema.json 里有未登记的封闭 enum（Rust 有镜像就必须登记）：{unregistered:?}"
    );

    // Rust 镜像的 `ALL`/`as_str` 序列必须等于登记序列（否则登记表与镜像又会各自漂移）。
    let registered_values = |event_type: &str, pointer: &str| -> Vec<&'static str> {
        views::VIEW_ENUMS
            .iter()
            .find(|(registered_type, registered_pointer, _)| {
                *registered_type == event_type && *registered_pointer == pointer
            })
            .map(|(_, _, values)| values.to_vec())
            .unwrap_or_else(|| panic!("{event_type}{pointer} 未登记"))
    };
    assert_eq!(
        views::PlanPriority::ALL
            .map(|value| value.as_str())
            .to_vec(),
        registered_values(
            "session.plan.changed",
            "/properties/entries/items/properties/priority"
        ),
    );
    assert_eq!(
        views::PlanStatus::ALL.map(|value| value.as_str()).to_vec(),
        registered_values(
            "session.plan.changed",
            "/properties/entries/items/properties/status"
        ),
    );
    assert_eq!(
        views::ElicitationAction::ALL
            .map(|value| value.as_str())
            .to_vec(),
        registered_values("elicitation.resolved", "/properties/action"),
    );
    assert_eq!(
        views::TerminalStream::ALL
            .map(|value| value.as_str())
            .to_vec(),
        registered_values("terminal.output", "/properties/stream"),
    );
}

#[test]
fn unknown_message_type_is_rejected() {
    assert!("auth.nonexistent".parse::<MessageType>().is_err());
    assert!("error".parse::<MessageType>().is_ok());
}

#[test]
fn error_codes_and_retryable_match_registry() {
    let registry = support::read_json(&support::repo_path("compatibility/errors/v1/errors.json"));
    let entries = registry["protocols"]["sync"]["errors"]
        .as_array()
        .expect("registry 的 sync.errors");

    let registered: Vec<(String, bool)> = entries
        .iter()
        .map(|entry| {
            (
                entry["code"].as_str().expect("code").to_owned(),
                entry["retryable"].as_bool().expect("retryable"),
            )
        })
        .collect();
    let declared: Vec<(String, bool)> = ErrorCode::ALL
        .iter()
        .map(|code| (code.as_str().to_owned(), code.default_retryable()))
        .collect();

    assert_eq!(
        declared.len(),
        34,
        "v1 的 sync 错误码应为 34 个（registry 有 {} 个）",
        registered.len()
    );
    assert_eq!(
        declared.len(),
        registered.len(),
        "错误码数量与 registry 不一致"
    );
    for (index, ((declared_code, declared_retryable), (registered_code, registered_retryable))) in
        declared.iter().zip(&registered).enumerate()
    {
        assert_eq!(
            declared_code, registered_code,
            "第 {index} 个错误码与 registry 顺序或字面量不一致"
        );
        assert_eq!(
            declared_retryable, registered_retryable,
            "{declared_code} 的默认 retryable 与 registry 不一致"
        );
    }

    for code in ErrorCode::ALL {
        let wire = serde_json::to_string(&code).expect("可序列化");
        assert_eq!(wire, format!("\"{}\"", code.as_str()));
        assert_eq!(code.as_str().parse::<ErrorCode>().expect("可解析"), code);
    }
}
