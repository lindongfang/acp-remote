//! 合同漂移门禁：Rust 里的消息类型、错误码与 grant 词表必须与 schema / registry / 命令目录逐条相等。
//!
//! 三侧都是机器资产（`schemas/node-link/v1/*.schema.json`、`compatibility/errors/v1/errors.json`、
//! `compatibility/commands/v1/commands.json`），任一侧单独改动都会让这里失败——这就是"实现不会静默
//! 落后于合同"的判据。消息类型的抽取规则与 `scripts/check-schema-fixtures.mjs` 的 `declaredTypes`
//! 一致：从 `message.schema.json` 的 union 出发，只沿 `$ref` 与 `allOf`/`oneOf`/`anyOf` 递归，
//! **不进入** `properties.*`。

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use node_link_protocol::common::{GrantName, TranscriptDomain};
use node_link_protocol::envelope::MessageType;
use node_link_protocol::error::ErrorCode;

/// `schemas/node-link/v1/` 下的全部 schema，按文件名索引。
struct SchemaSet {
    documents: BTreeMap<String, serde_json::Value>,
}

impl SchemaSet {
    fn load(root: &Path) -> Self {
        let mut documents = BTreeMap::new();
        for entry in std::fs::read_dir(root).expect("schemas/node-link/v1 目录") {
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
            if let Some(reference) = map.get("$ref").and_then(|value| value.as_str()) {
                let (referenced_file, pointer) = match reference.split_once('#') {
                    Some((file, pointer)) => (file.to_owned(), pointer.to_owned()),
                    None => (reference.to_owned(), String::new()),
                };
                let next_file = if referenced_file.is_empty() {
                    file.to_owned()
                } else {
                    referenced_file
                };
                let document = set.document(&next_file);
                let target = if pointer.is_empty() {
                    document
                } else {
                    lookup_pointer(document, &pointer)
                        .unwrap_or_else(|| panic!("{next_file} 里没有指针 {pointer}"))
                };
                collect_declared_types(set, &next_file, target, found, depth + 1);
            }
            for keyword in ["allOf", "oneOf", "anyOf"] {
                if let Some(branches) = map.get(keyword).and_then(|value| value.as_array()) {
                    for branch in branches {
                        collect_declared_types(set, file, branch, found, depth + 1);
                    }
                }
            }
            if let Some(constant) = map
                .get("properties")
                .and_then(|properties| properties.get("type"))
                .and_then(|property| property.get("const"))
                .and_then(|constant| constant.as_str())
            {
                found.insert(constant.to_owned());
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
    let set = SchemaSet::load(&support::repo_path("schemas/node-link/v1"));
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

    let missing: Vec<&String> = expected.difference(&declared).collect();
    let extra: Vec<&String> = declared.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "schema 有而 Rust 没有：{missing:?}；Rust 有而 schema 没有：{extra:?}"
    );
    assert_eq!(
        declared.len(),
        expected.len(),
        "消息类型数与 schema 抽取结果不一致"
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
fn unknown_message_type_is_rejected() {
    assert!("node.nonexistent".parse::<MessageType>().is_err());
    assert!("link.error".parse::<MessageType>().is_ok());
}

#[test]
fn error_codes_and_retryable_match_registry() {
    let registry = support::read_json(&support::repo_path("compatibility/errors/v1/errors.json"));
    let entries = registry["protocols"]["node_link"]["errors"]
        .as_array()
        .expect("registry 的 node_link.errors");

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
        registered.len(),
        "Node Link 错误码数与 registry 不一致（registry {} 个、Rust {} 个）",
        registered.len(),
        declared.len()
    );
    for (declared_entry, registered_entry) in declared.iter().zip(registered.iter()) {
        assert_eq!(
            declared_entry, registered_entry,
            "错误码及其 retryable 必须与 registry 逐条相等"
        );
    }
}

#[test]
fn grant_names_match_the_command_catalog() {
    let catalog = support::read_json(&support::repo_path(
        "compatibility/commands/v1/commands.json",
    ));
    let grants = catalog["grants"]
        .as_object()
        .expect("commands.json 的 grants");

    // JSON object 的键序不是合同的一部分，schema 的 `enum` 顺序才是——这里比较集合，枚举顺序与取值
    // 的字面量由 `common.rs` 的单测钉住。
    let registered: BTreeSet<&str> = grants.keys().map(String::as_str).collect();
    let declared: BTreeSet<&str> = GrantName::ALL.iter().map(|grant| grant.as_str()).collect();

    assert_eq!(
        declared, registered,
        "grant 词表的唯一来源是 commands.json：Rust 枚举必须与它逐条相等"
    );
    assert_eq!(declared.len(), 5, "grant 词表的项数变化");
}

#[test]
fn transcript_domains_match_the_registry() {
    let registered: Vec<String> = support::registry_domains("node_link")
        .iter()
        .map(|entry| entry["domain"].as_str().expect("domain").to_owned())
        .collect();
    let declared: Vec<&str> = TranscriptDomain::ALL
        .iter()
        .map(|domain| domain.as_str())
        .collect();

    assert_eq!(
        declared.len(),
        registered.len(),
        "domain 数与 registry 不一致"
    );
    for domain in &declared {
        assert!(
            registered.iter().any(|entry| entry == domain),
            "{domain} 不在 registry 的 node_link domain 列表里"
        );
    }
    assert_eq!(
        TranscriptDomain::ALL.len(),
        node_link_protocol::domains::DOMAINS.len(),
        "domain 枚举必须与 transcript 表逐条相等"
    );
}
