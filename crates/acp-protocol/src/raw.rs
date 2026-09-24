//! 原文承载：`RawDocument` 与 JSON-RPC `id` 的字面量。
//!
//! 这是本 crate 保真的**唯一**机制：解析只做「长度上限 + 是合法 JSON object」的检查，
//! 之后原文一字不改地留着，再编码就是回写原文。类型化 DTO 需要字段时按需解析，
//! 但解析结果只用于读值，不作为回写来源。

use serde_json::Value;

use crate::error::{AcpError, Result};
use crate::limits::MAX_MESSAGE_BYTES;

/// 一条 ACP wire 消息的原始 JSON document。
///
/// 不变量：文本是合法 JSON object，且长度不超过 [`MAX_MESSAGE_BYTES`]。**不含** stdio 分帧的换行
/// （`docs/SYNC_PROTOCOL.md` §10.3 对 `rawJson` 的同一约定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDocument {
    text: String,
}

impl RawDocument {
    /// 从一段文本构造。超过固定上限时返回 [`AcpError::Oversize`]，非法 JSON 返回
    /// [`AcpError::Json`]，顶层不是对象返回 [`AcpError::Malformed`]。
    pub fn parse(text: impl Into<String>) -> Result<Self> {
        let text = text.into();
        Self::check_size(text.len())?;
        let value: Value = serde_json::from_str(&text).map_err(|error| AcpError::Json {
            detail: error.to_string(),
        })?;
        if !value.is_object() {
            return Err(AcpError::Malformed {
                detail: "顶层不是 JSON 对象".to_owned(),
            });
        }
        Ok(Self { text })
    }

    /// 从字节构造；非 UTF-8 返回 [`AcpError::Malformed`]。
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self> {
        Self::check_size(bytes.len())?;
        let text = std::str::from_utf8(bytes).map_err(|error| AcpError::Malformed {
            detail: format!("不是合法 UTF-8：{error}"),
        })?;
        Self::parse(text)
    }

    fn check_size(len: usize) -> Result<()> {
        if len > MAX_MESSAGE_BYTES {
            return Err(AcpError::Oversize {
                limit: MAX_MESSAGE_BYTES,
                actual: len,
            });
        }
        Ok(())
    }

    /// 原文。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// 再编码的字节。**就是原文**——这是保真的实现，不是巧合。
    #[must_use]
    pub fn encode(&self) -> &str {
        &self.text
    }

    /// 原文字节。
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }

    /// 原文的 UTF-8 字节长度。
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// 原文是否为空（不变量保证不会发生；为满足 `clippy::len_without_is_empty` 而提供）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// 按需解析成 [`Value`]。返回值只用于**读取字段**，不是回写来源。
    pub fn value(&self) -> Result<Value> {
        serde_json::from_str(&self.text).map_err(|error| AcpError::Json {
            detail: error.to_string(),
        })
    }

    /// 顶层对象的成员；不是对象或没有该键时返回 `None`。
    pub fn member(&self, key: &str) -> Result<Option<Value>> {
        Ok(self
            .value()?
            .as_object()
            .and_then(|object| object.get(key))
            .cloned())
    }

    /// 顶层成员在原文中的**字面量切片**（含引号/数字文本原样，不含前后空白）。
    ///
    /// 用来按原始类型与字面量回填 `id` 等字段：`Value` 会把 `"\u0041"` 规范化成 `"A"`，
    /// 那不是「原文保真」。
    #[must_use]
    pub fn member_literal(&self, key: &str) -> Option<&str> {
        top_level_member_slice(&self.text, key)
    }
}

/// 顶层对象成员的原始切片。
///
/// 假定输入已经过 [`RawDocument::parse`] 的校验（合法 JSON object），因此只做结构扫描，
/// 不重新做语法校验。索引只落在 ASCII 结构位置上，切片不会切进多字节字符。
fn top_level_member_slice<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let bytes = text.as_bytes();
    let mut cursor = skip_whitespace(bytes, 0);
    if bytes.get(cursor) != Some(&b'{') {
        return None;
    }
    cursor += 1;
    loop {
        cursor = skip_whitespace(bytes, cursor);
        match bytes.get(cursor)? {
            b',' => {
                cursor += 1;
            }
            b'}' => return None,
            b'"' => {
                let name_end = scan_string(bytes, cursor)?;
                let name = text.get(cursor + 1..name_end.checked_sub(1)?)?;
                cursor = skip_whitespace(bytes, name_end);
                if bytes.get(cursor) != Some(&b':') {
                    return None;
                }
                cursor = skip_whitespace(bytes, cursor + 1);
                let value_end = scan_value(bytes, cursor)?;
                if name == key {
                    return text.get(cursor..value_end);
                }
                cursor = value_end;
            }
            _ => return None,
        }
    }
}

fn skip_whitespace(bytes: &[u8], mut cursor: usize) -> usize {
    while let Some(byte) = bytes.get(cursor) {
        if byte.is_ascii_whitespace() {
            cursor += 1;
        } else {
            break;
        }
    }
    cursor
}

/// 从开引号扫描到闭引号之后的位置。
fn scan_string(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start + 1;
    let mut escaped = false;
    while let Some(byte) = bytes.get(cursor) {
        if escaped {
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else if *byte == b'"' {
            return Some(cursor + 1);
        }
        cursor += 1;
    }
    None
}

/// 从值的起点扫描到值的结尾之后。
fn scan_value(bytes: &[u8], start: usize) -> Option<usize> {
    match bytes.get(start)? {
        b'"' => scan_string(bytes, start),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut cursor = start;
            let mut in_string = false;
            let mut escaped = false;
            while let Some(byte) = bytes.get(cursor) {
                if in_string {
                    if escaped {
                        escaped = false;
                    } else if *byte == b'\\' {
                        escaped = true;
                    } else if *byte == b'"' {
                        in_string = false;
                    }
                } else {
                    match byte {
                        b'"' => in_string = true,
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth = depth.checked_sub(1)?;
                            if depth == 0 {
                                return Some(cursor + 1);
                            }
                        }
                        _ => {}
                    }
                }
                cursor += 1;
            }
            None
        }
        _ => {
            let mut cursor = start;
            while let Some(byte) = bytes.get(cursor) {
                if matches!(byte, b',' | b'}' | b']') {
                    break;
                }
                cursor += 1;
            }
            let mut end = cursor;
            while end > start && bytes.get(end - 1).is_some_and(u8::is_ascii_whitespace) {
                end -= 1;
            }
            Some(end)
        }
    }
}

/// JSON-RPC `id` 的字面量。
///
/// 上游客户端的 `id` **不保证是整数**（`docs/ACP_COMPATIBILITY_MATRIX.md` §6 实测 Zed 使用 UUID
/// 字符串），因此这里同时保留字面量文本与类型判别：回填时按原类型与字面量输出，不转成整数、
/// 不重新编号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdLiteral {
    /// 数字（保留原始数字文本，例如超长整数）。
    Number(String),
    /// 字符串（保留包含引号的原始 JSON 文本，转义不被规范化）。
    String(String),
    /// 布尔（ACS 未使用，但 JSON-RPC 允许）。
    Bool(String),
    /// `null`。
    Null,
}

impl IdLiteral {
    /// 从原文切片构造。切片必须是合法 JSON 标量。
    pub fn from_literal(literal: &str) -> Result<Self> {
        let trimmed = literal.trim();
        match trimmed.as_bytes().first() {
            Some(b'"') => Ok(Self::String(trimmed.to_owned())),
            Some(b't' | b'f') => Ok(Self::Bool(trimmed.to_owned())),
            Some(b'n') => Ok(Self::Null),
            Some(byte) if byte.is_ascii_digit() || *byte == b'-' => {
                serde_json::from_str::<Value>(trimmed).map_err(|error| AcpError::InvalidField {
                    field: "id".to_owned(),
                    detail: error.to_string(),
                })?;
                Ok(Self::Number(trimmed.to_owned()))
            }
            _ => Err(AcpError::InvalidField {
                field: "id".to_owned(),
                detail: "不是 JSON 标量".to_owned(),
            }),
        }
    }

    /// 按原类型与字面量输出的 JSON 文本。
    #[must_use]
    pub fn as_json(&self) -> &str {
        match self {
            Self::Number(text) | Self::String(text) | Self::Bool(text) => text,
            Self::Null => "null",
        }
    }

    /// 数字字面量的文本（仅 [`IdLiteral::Number`]）。
    #[must_use]
    pub fn number_text(&self) -> Option<&str> {
        match self {
            Self::Number(text) => Some(text),
            _ => None,
        }
    }

    /// 字符串字面量的**解码后**内容（仅 [`IdLiteral::String`]）。
    #[must_use]
    pub fn string_value(&self) -> Option<String> {
        match self {
            Self::String(text) => serde_json::from_str::<String>(text).ok(),
            _ => None,
        }
    }

    /// 是否与某个 `Value` 表示同一字面量（用于把响应匹配回未完成请求）。
    #[must_use]
    pub fn matches_value(&self, value: &Value) -> bool {
        match (self, value) {
            (Self::Null, Value::Null) => true,
            (Self::Bool(text), Value::Bool(flag)) => text == if *flag { "true" } else { "false" },
            (Self::String(_), Value::String(other)) => {
                self.string_value().is_some_and(|mine| mine == *other)
            }
            (Self::Number(_), Value::Number(other)) => {
                self.number_text() == Some(other.to_string().as_str())
            }
            _ => false,
        }
    }
}
