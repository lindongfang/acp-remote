//! elicitation 的动作与 `values` 容器（`docs/CORE_PORTS_AND_STORAGE.md` §3.3）。
//!
//! A1 定案后的**定形**实现，形状对齐 ACP 的 `ElicitationContentValue`：
//!
//! - [`ElicitationContentValue`] 是单个值的六形态；未知的未来 ACP 变体走 `Unknown`，保存该值的 JSON
//!   **原文**并原样转发，core 不解释它（`json::JsonValueText`，不是 `serde_json::Value`）。
//! - [`ElicitationValues`] 是容器：ACP 的 `content` 既可以是 `null` 也可以是 object，且「`{}`（空对象）」
//!   与「`null`」必须可区分（`is_null()` / `is_empty_object()`）。
//! - 边界 ＝ 合同 §3.3 已冻结的两条：**规范序列化后 ≤64 KiB、深度 ≤16**。所有构造入口与
//!   `FromStr`（反序列化入口）都执行这两条校验，越界返回具名 [`InvalidValue`]。
//! - 语义（§11.5 + A1 定案）：`submit` ⇒ `null` 或 object 皆可；`cancel`/`decline` ⇒ 必须是 `null`。
//!   该规则由 `InteractionResolution::validate` 与 `CommandPayload::validate` 共用。
//!
//! core 没有 serde（`Cargo.toml` 只有 async-trait + thiserror），所以「反序列化」的入口是
//! [`ElicitationValues::parse_json`] / `FromStr`：协议 crate 解码 `content` 时调用它，从而对所有入口都
//! 成立同样的边界校验。

use std::fmt;
use std::str::FromStr;

use super::error::InvalidValue;
use super::ids::require_bounded;
use super::json::{
    JsonValueText, decode_json_string, encode_json_string, object_members, string_array_items,
    validate_document,
};

token_enum!(
    /// elicitation 的动作（`SYNC_PROTOCOL.md` §11.5；`decline` 由 A1 定案加入，与协议 crate 同步）。
    ElicitationAction {
        Submit => "submit",
        Cancel => "cancel",
        Decline => "decline",
    }
);

/// 单个 elicitation 取值（对齐 ACP `ElicitationContentValue`）。
#[derive(Debug, Clone, PartialEq)]
pub enum ElicitationContentValue {
    /// ACP string（长度由容器总字节上限约束）。
    Text(String),
    /// ACP integer(int64)：必须精确，不退化成 `f64`；超出 `i64` 的整数字面量归入 `Unknown`。
    Integer(i64),
    /// ACP number(double)；非有限值（NaN/±∞）不是合法 JSON，构造时被拒。
    Number(f64),
    /// ACP boolean。
    Boolean(bool),
    /// ACP string[]。
    TextArray(Vec<String>),
    /// 未来 ACP 变体：保存该值的 JSON 原文，原样保留、原样转发，不解释。
    Unknown(JsonValueText),
}

impl ElicitationContentValue {
    /// 未知变体：`raw` 必须是良构 JSON 值（单个值，不是文档片段拼装）。
    pub fn unknown(raw: &str) -> Result<Self, InvalidValue> {
        Ok(Self::Unknown(JsonValueText::new(raw)?))
    }

    /// 数值：非有限值被拒绝（JSON 没有 NaN/Infinity）。
    pub fn number(value: f64) -> Result<Self, InvalidValue> {
        if value.is_finite() {
            Ok(Self::Number(value))
        } else {
            Err(InvalidValue::Json)
        }
    }

    /// 该值是否属于 ACP 已登记的五种形态（`Unknown` 之外）。
    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }

    /// 规范序列化（`Unknown` 输出其原文）。
    fn write_json(&self, out: &mut String) {
        match self {
            Self::Text(text) => out.push_str(&encode_json_string(text)),
            Self::Integer(value) => out.push_str(&value.to_string()),
            Self::Number(value) => out.push_str(&format_number(*value)),
            Self::Boolean(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::TextArray(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    out.push_str(&encode_json_string(item));
                }
                out.push(']');
            }
            Self::Unknown(raw) => out.push_str(raw.as_str()),
        }
    }
}

/// `f64` 的 JSON 文本：保证带小数点或指数，使 `Number` 往返后仍是 `Number`（而不是被读成 `Integer`）。
fn format_number(value: f64) -> String {
    let text = format!("{value}");
    if text.contains(['.', 'e', 'E']) {
        text
    } else {
        format!("{text}.0")
    }
}

/// elicitation 的 `content` 容器：有序的 键 → 值。
///
/// 三态：
///
/// - [`ElicitationValues::null`] —— ACP `content: null`；
/// - [`ElicitationValues::empty_object`] —— ACP `content: {}`；
/// - 非空字段列表 —— ACP `content: {…}`。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ElicitationValues(ElicitationState);

#[derive(Debug, Clone, PartialEq, Default)]
enum ElicitationState {
    #[default]
    Null,
    Object(Vec<(String, ElicitationContentValue)>),
}

impl ElicitationValues {
    /// 规范序列化后的最大字节数（64 KiB）。
    pub const MAX_BYTES: usize = 64 * 1024;
    /// 最大嵌套深度。
    pub const MAX_DEPTH: u16 = 16;

    /// ACP `content: null`。
    pub fn null() -> Self {
        Self::default()
    }

    /// ACP `content: {}`。
    pub fn empty_object() -> Self {
        Self(ElicitationState::Object(Vec::new()))
    }

    /// 从字段构造；键不得重复且必须是非空字符串，序列化后必须满足 64 KiB / 深度 16。
    pub fn from_fields<I: IntoIterator<Item = (String, ElicitationContentValue)>>(
        fields: I,
    ) -> Result<Self, InvalidValue> {
        let fields: Vec<(String, ElicitationContentValue)> = fields.into_iter().collect();
        for (index, (key, _)) in fields.iter().enumerate() {
            require_bounded(key, 1, Self::MAX_BYTES)?;
            if fields[..index].iter().any(|(other, _)| other == key) {
                return Err(InvalidValue::Field);
            }
        }
        Self(ElicitationState::Object(fields)).validated()
    }

    /// 解析 ACP `content` 的 JSON 原文（`null` 或 object）；重复键、非 object/null 顶层、越界都被拒绝。
    pub fn parse_json(text: &str) -> Result<Self, InvalidValue> {
        validate_document(text, false)?;
        let trimmed = text.trim_start();
        if trimmed.starts_with("null") {
            return Ok(Self::null());
        }
        if !trimmed.starts_with('{') {
            return Err(InvalidValue::Json);
        }
        let mut fields = Vec::new();
        for (key, raw) in object_members(text)? {
            if fields
                .iter()
                .any(|(other, _): &(String, ElicitationContentValue)| *other == key)
            {
                return Err(InvalidValue::Field);
            }
            fields.push((key, classify(raw)?));
        }
        Self(ElicitationState::Object(fields)).validated()
    }

    /// 是否为 ACP `content: null`。
    pub fn is_null(&self) -> bool {
        matches!(self.0, ElicitationState::Null)
    }

    /// 是否为 ACP `content: {}`（空对象，与 `null` 不同）。
    pub fn is_empty_object(&self) -> bool {
        matches!(&self.0, ElicitationState::Object(fields) if fields.is_empty())
    }

    /// 字段列表；`null` 时返回 `None`。
    pub fn fields(&self) -> Option<&[(String, ElicitationContentValue)]> {
        match &self.0 {
            ElicitationState::Null => None,
            ElicitationState::Object(fields) => Some(fields),
        }
    }

    /// 按键取值；`null` 时返回 `None`。
    pub fn get(&self, key: &str) -> Option<&ElicitationContentValue> {
        self.fields()?
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// 规范序列化（键保持构造顺序，`Unknown` 输出原文）。这是写入持久层的文本。
    pub fn to_json_text(&self) -> String {
        match &self.0 {
            ElicitationState::Null => "null".to_owned(),
            ElicitationState::Object(fields) => {
                let mut out = String::from("{");
                for (index, (key, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    out.push_str(&encode_json_string(key));
                    out.push(':');
                    value.write_json(&mut out);
                }
                out.push('}');
                out
            }
        }
    }

    /// 校验 64 KiB / 深度 16 两条边界（读取路径与用例层都可以再调一次）。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        self.clone().validated().map(|_| ())
    }

    /// 自校验：序列化 → 长度与深度检查（深度用同一套 JSON 校验器量，序列化器自身也顺带被验证）。
    fn validated(self) -> Result<Self, InvalidValue> {
        let text = self.to_json_text();
        if text.len() > Self::MAX_BYTES {
            return Err(InvalidValue::TooLarge {
                max: Self::MAX_BYTES,
            });
        }
        let depth = validate_document(&text, false)?;
        if depth > Self::MAX_DEPTH {
            return Err(InvalidValue::Depth {
                max: Self::MAX_DEPTH,
            });
        }
        Ok(self)
    }
}

impl fmt::Display for ElicitationValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_json_text())
    }
}

impl FromStr for ElicitationValues {
    type Err = InvalidValue;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse_json(text)
    }
}

/// 把一个 JSON 值原文归类成 ACP 取值；无法归类（未来变体、超出 `i64` 的大整数、非全字符串数组、null、
/// object）一律归入 `Unknown` 并保留原文。
fn classify(raw: &str) -> Result<ElicitationContentValue, InvalidValue> {
    match raw.as_bytes().first() {
        Some(b'"') => match decode_json_string(raw) {
            Some(text) => Ok(ElicitationContentValue::Text(text)),
            None => Ok(ElicitationContentValue::Unknown(JsonValueText::new(raw)?)),
        },
        Some(b't') | Some(b'f') => Ok(ElicitationContentValue::Boolean(raw == "true")),
        Some(b'[') => match string_array_items(raw) {
            Some(items) => Ok(ElicitationContentValue::TextArray(items)),
            None => Ok(ElicitationContentValue::Unknown(JsonValueText::new(raw)?)),
        },
        Some(b'-' | b'0'..=b'9') => {
            if raw.contains(['.', 'e', 'E']) {
                match raw.parse::<f64>() {
                    Ok(value) if value.is_finite() => Ok(ElicitationContentValue::Number(value)),
                    _ => Ok(ElicitationContentValue::Unknown(JsonValueText::new(raw)?)),
                }
            } else {
                // 整数字面量必须精确：超出 i64 归入 Unknown，不退化成 f64。
                match raw.parse::<i64>() {
                    Ok(value) => Ok(ElicitationContentValue::Integer(value)),
                    Err(_) => Ok(ElicitationContentValue::Unknown(JsonValueText::new(raw)?)),
                }
            }
        }
        _ => Ok(ElicitationContentValue::Unknown(JsonValueText::new(raw)?)),
    }
}
