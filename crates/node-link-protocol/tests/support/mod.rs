//! 测试共用辅助：定位仓库根、读取 registry 与 manifest、把 fixture 的 `input` 按 registry 的
//! `inputKey` 约定转换成字段字节，以及把错误映射成合同字符串。
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use acpr_transcript::table::TableError;
use serde_json::Value;

/// registry 在仓库中的相对路径（12 个 domain 的机器登记）。
pub const REGISTRY_PATH: &str = "compatibility/transcripts/v1/transcripts.json";

/// 仓库根下的路径。集成测试的 cwd 不保证是仓库根，因此一律用 `CARGO_MANIFEST_DIR` 定位。
pub fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative)
}

pub fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("无法读取 {}：{error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("无法解析 {}：{error}", path.display()))
}

pub fn registry() -> Value {
    read_json(&repo_path(REGISTRY_PATH))
}

/// registry 中 `protocol == protocol` 的 domain 条目，顺序与 registry 一致。
pub fn registry_domains(protocol: &str) -> Vec<Value> {
    registry()["domains"]
        .as_array()
        .expect("registry.domains")
        .iter()
        .filter(|entry| entry["protocol"].as_str() == Some(protocol))
        .cloned()
        .collect()
}

pub fn registry_domain_entry(registry: &Value, domain: &str) -> Option<Value> {
    registry["domains"]
        .as_array()?
        .iter()
        .find(|entry| entry["domain"].as_str() == Some(domain))
        .cloned()
}

/// manifest 里 `transcriptVectors` 列出的向量路径（相对该 fixture root）。
pub fn vectors(manifest_relative: &str) -> Vec<String> {
    read_json(&repo_path(manifest_relative))["transcriptVectors"]
        .as_array()
        .expect("transcriptVectors")
        .iter()
        .map(|entry| entry.as_str().expect("vector path").to_string())
        .collect()
}

pub fn hex_decode(text: &str) -> Vec<u8> {
    assert!(text.len() % 2 == 0, "hex 输入长度为奇数：{text}");
    (0..text.len() / 2)
        .map(|index| {
            u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
                .unwrap_or_else(|error| panic!("hex 解析失败：{error}"))
        })
        .collect()
}

fn uuid_bytes(text: &str) -> Vec<u8> {
    let compact: String = text.chars().filter(|character| *character != '-').collect();
    assert_eq!(compact.len(), 32, "uuid 不是 32 个十六进制字符：{text}");
    hex_decode(&compact)
}

/// 按 registry 的 `inputKey` 约定把 fixture 的 `input` 值转成字段字节。
///
/// 键名以 `Hex` 结尾的二进制字段按十六进制解析，其余二进制字段按无填充 base64url 解析；
/// `u16be`/`u64be` 取整数，`uuid16` 取 UUID 文本，`utf8` 取字符串，`nul-joined-utf8`
/// 取字符串数组。
pub fn input_bytes(input: &Value, field: &Value) -> Vec<u8> {
    let input_key = field["inputKey"].as_str().expect("inputKey");
    let field_type = field["type"].as_str().expect("type");
    let value = &input[input_key];

    match field_type {
        "u16be" => (value.as_u64().expect("u16be value") as u16)
            .to_be_bytes()
            .to_vec(),
        "u64be" => value.as_u64().expect("u64be value").to_be_bytes().to_vec(),
        "uuid16" => uuid_bytes(value.as_str().expect("uuid text")),
        "utf8" => value.as_str().expect("utf8 value").as_bytes().to_vec(),
        "nul-joined-utf8" => {
            let parts: Vec<&str> = value
                .as_array()
                .expect("nul-joined array")
                .iter()
                .map(|part| part.as_str().expect("feature id"))
                .collect();
            acpr_transcript::encode_nul_joined(&parts).expect("nul-joined 编码")
        }
        "sec1-65" | "bytes16" | "bytes32" => {
            let text = value.as_str().expect("binary field text");
            if input_key.ends_with("Hex") {
                hex_decode(text)
            } else {
                acpr_transcript::decode_base64url(text).expect("base64url 字段")
            }
        }
        other => panic!("registry 里出现未知字段类型：{other}"),
    }
}

/// 把 codec 层错误映射成 `fixtures/*/v1/transcripts/invalid/*.json` 的 `expectedError` 取值。
pub fn codec_error_name(error: &acpr_transcript::TranscriptError) -> &'static str {
    match error {
        acpr_transcript::TranscriptError::Base64Url => "bad_base64url",
        acpr_transcript::TranscriptError::InvalidDomain => "invalid_domain",
        acpr_transcript::TranscriptError::BadMagic => "bad_magic",
        acpr_transcript::TranscriptError::BadCodecVersion { .. } => "bad_codec_version",
        acpr_transcript::TranscriptError::TruncatedDomain => "truncated_domain",
        acpr_transcript::TranscriptError::TruncatedField => "truncated_field",
        acpr_transcript::TranscriptError::NotUtf8 => "not_utf8",
        acpr_transcript::TranscriptError::DuplicateTag { .. } => "duplicate_tag",
        acpr_transcript::TranscriptError::FieldOrder { .. } => "field_order",
        acpr_transcript::TranscriptError::TrailingBytes => "trailing_bytes",
        acpr_transcript::TranscriptError::TooManyFields => "too_many_fields",
        acpr_transcript::TranscriptError::FieldTooLong => "field_too_long",
        acpr_transcript::TranscriptError::EmbeddedNul => "embedded_nul",
    }
}

/// 把表驱动层错误映射成同一组字符串。
pub fn error_name(error: &TableError) -> &'static str {
    match error {
        TableError::Codec(inner) => codec_error_name(inner),
        TableError::UnknownDomain(_) => "unknown_domain",
        TableError::UnknownTag { .. } => "unknown_tag",
        TableError::LengthMismatch { .. } => "length_mismatch",
        TableError::FieldCountMismatch { .. } => "field_count_mismatch",
    }
}

/// `fixtures/node-link/v1/manifest.json` 的一条用例。
#[derive(Debug, Clone)]
pub struct ManifestCase {
    pub fixture: String,
    pub schema: String,
    pub valid: bool,
    pub expected_keyword: Option<String>,
    /// 事件视图夹具声明的 `payload.view` 绑定（`#/$defs/<eventType>`，指向 Sync 的视图定义）。
    pub view_def: Option<String>,
}

pub fn manifest_cases(manifest_relative: &str) -> Vec<ManifestCase> {
    read_json(&repo_path(manifest_relative))["cases"]
        .as_array()
        .expect("manifest.cases")
        .iter()
        .map(|case| ManifestCase {
            fixture: case["fixture"].as_str().expect("fixture").to_owned(),
            schema: case["schema"].as_str().expect("schema").to_owned(),
            valid: case["valid"].as_bool().expect("valid"),
            expected_keyword: case["expectedKeyword"]
                .as_str()
                .map(|keyword| keyword.to_owned()),
            view_def: case["viewDef"].as_str().map(|def| def.to_owned()),
        })
        .collect()
}

pub fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("无法读取 {}：{error}", path.display()))
}

/// 从消息文本里取出 `body` 的原始切片（字符串感知的括号配对，不做 JSON 解析）。
///
/// 用来证明信封层没有把 body 解析成通用 DTO 再重新序列化：切片必须逐字节相等。
pub fn body_slice(text: &str) -> &str {
    let bytes = text.as_bytes();
    let key = "\"body\"";

    let mut search_from = 0;
    let start = loop {
        let found = text[search_from..]
            .find(key)
            .unwrap_or_else(|| panic!("消息里没有 body 字段：{text}"))
            + search_from;
        let mut cursor = found + key.len();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b':' {
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            break cursor;
        }
        search_from = found + key.len();
    };

    match bytes[start] {
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut cursor = start;
            let mut in_string = false;
            let mut escaped = false;
            while cursor < bytes.len() {
                let byte = bytes[cursor];
                if in_string {
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        in_string = false;
                    }
                } else {
                    match byte {
                        b'"' => in_string = true,
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth -= 1;
                            if depth == 0 {
                                return &text[start..=cursor];
                            }
                        }
                        _ => {}
                    }
                }
                cursor += 1;
            }
            panic!("body 未闭合：{text}")
        }
        _ => {
            let mut cursor = start;
            while cursor < bytes.len() && !b",}]".contains(&bytes[cursor]) {
                cursor += 1;
            }
            text[start..cursor].trim_end()
        }
    }
}
