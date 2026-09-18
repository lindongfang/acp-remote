//! 跨协议共用的 wire 值对象与反序列化期校验（叶子 crate）。
//!
//! 这里只放与具体协议线无关的东西：`uuid`、`decimalString`、`featureId`/`featureList`、
//! 无填充 base64url 的定长二进制、有界整数、长度受限文本、`T | null` 字段、`timestamp`、
//! `rawAcp`、原始 JSON 对象与未知字段载体，以及它们的取值校验错误 [`ValueError`]。
//!
//! [`cj1`] 是跨 Sync/Node Link 共用的 ACPR-CJ1 规范 JSON（`payloadDigest`/`snapshotDigest` 的前像）。
//!
//! 本 crate **不拥有任何协议语义**：没有命令名、事件名、domain/tag 表，也没有错误码词表——
//! [`PublicError`] 的 code 因此是类型参数，由使用方绑定自己那条线的错误码类型。依赖方向是单向
//! 的：协议 crate 依赖本 crate，本 crate 只依赖 `acpr-transcript` 的 base64url 编解码。
//!
//! 校验只发生在反序列化：构造出来的值必然满足对应 schema，后续层不需要重复校验。

use std::collections::BTreeMap;

use serde::de::{DeserializeOwned, Error as DeError};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

pub mod cj1;

/// 公共类型的取值校验错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValueError {
    #[error("uuid 必须是 8-4-4-4-12 的小写十六进制：{0}")]
    Uuid(String),
    #[error("decimalString 必须是无前导零的十进制字符串：{0}")]
    DecimalString(String),
    #[error("featureId 必须是 1..=64 个 [a-z0-9.-]：{0}")]
    FeatureId(String),
    #[error("featureList 最多 64 项，实际 {0}")]
    FeatureListTooLong(usize),
    #[error("featureList 含重复项：{0}")]
    FeatureListDuplicate(String),
    #[error("base64url 不是规范的无填充编码：{0}")]
    Base64Url(String),
    #[error("base64url 解码后应为 {expected} 字节，实际 {actual}")]
    Base64UrlLength { expected: usize, actual: usize },
    #[error("整数超出允许范围 [{min}, {max}]：{value}")]
    IntegerRange { min: u64, max: u64, value: u64 },
    #[error("该字段必须是 JSON object")]
    NotAnObject,
    #[error("未知错误码：{0}")]
    UnknownErrorCode(String),
    #[error("{field} 是 required 字段，键必须存在")]
    MissingField { field: &'static str },
    #[error("{field} 不在已登记的取值集合里：{value}")]
    Enumerated { field: &'static str, value: String },
    #[error("{field} 含重复项：{value}")]
    RepeatedItem { field: &'static str, value: String },
    /// 协议侧那些"按 schema 的 `pattern` 校验的字符串 newtype"共用的失败形状：本条线的
    /// ValueError 只登记跨线通用的取值域，具体 pattern 由调用方在 `field` 里点名。
    #[error("{field} 不符合已登记的 pattern：{value}")]
    Pattern { field: &'static str, value: String },
    #[error("文本长度必须在 {min}..={max} 之间，实际 {actual}")]
    TextLength {
        min: usize,
        max: usize,
        actual: usize,
    },
    #[error("canonicalOrigin 必须是 https://<authority>，且不含路径、查询、片段或空白：{0}")]
    CanonicalOrigin(String),
    #[error("timestamp 必须是 YYYY-MM-DDTHH:MM:SS.mmmZ：{0}")]
    Timestamp(String),
    #[error("{field} 的 JSON 形状不符合该位置允许的形状：{expected}")]
    Shape {
        field: &'static str,
        expected: &'static str,
    },
    #[error("{field} 超过 {max} 项，实际 {actual}")]
    TooManyItems {
        field: &'static str,
        max: usize,
        actual: usize,
    },
    #[error("{field} 不符合已登记的 JSON 形状：{detail}")]
    ViewShape { field: &'static str, detail: String },
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

/// 把已确认是 ASCII 数字的字节折成数值（调用方负责保证每一位都是数字）。
fn digits(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0, |value, byte| value * 10 + u32::from(byte - b'0'))
}

/// 闰年判定（RFC 3339 附录 C 的规则，与 ajv-formats 的 `isLeapYear` 相同）。
fn is_leap_year(year: u32) -> bool {
    // 先取余再比较：直接写 `year % 4 == 0 && …` 会被 clippy::manual_is_multiple_of 建议改用
    // `u32::is_multiple_of`，而那个 API 稳定于 Rust 1.87，本 workspace 的 `rust-version` 是 1.85。
    let by_four = year % 4;
    let by_hundred = year % 100;
    let by_four_hundred = year % 400;
    by_four == 0 && (by_hundred != 0 || by_four_hundred == 0)
}

/// `uuid`：8-4-4-4-12 的小写十六进制。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uuid(String);

impl Uuid {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let bytes = text.as_bytes();
        let well_formed = bytes.len() == 36
            && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || is_lower_hex(*byte));
        if !well_formed {
            return Err(ValueError::Uuid(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for Uuid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Uuid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `decimalString`：无前导零的十进制字符串。序列号本身就是字符串，不得中途转成浮点或机器整数。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DecimalString(String);

impl DecimalString {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let bytes = text.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.iter().all(u8::is_ascii_digit)
            && (bytes == b"0" || bytes[0] != b'0');
        if !valid {
            return Err(ValueError::DecimalString(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for DecimalString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DecimalString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `featureId`：`[a-z0-9.-]`，1..=64 字符。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FeatureId(String);

impl FeatureId {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let valid = !text.is_empty()
            && text.len() <= 64
            && text.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'-'
            });
        if !valid {
            return Err(ValueError::FeatureId(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for FeatureId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for FeatureId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `featureList`：最多 64 项、去重。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeatureList(Vec<FeatureId>);

impl FeatureList {
    pub fn new(features: Vec<FeatureId>) -> Result<Self, ValueError> {
        let list = Self(features);
        list.validate()?;
        Ok(list)
    }

    pub fn as_slice(&self) -> &[FeatureId] {
        &self.0
    }

    pub fn contains(&self, feature: &FeatureId) -> bool {
        self.0.contains(feature)
    }

    fn validate(&self) -> Result<(), ValueError> {
        if self.0.len() > 64 {
            return Err(ValueError::FeatureListTooLong(self.0.len()));
        }
        for (index, feature) in self.0.iter().enumerate() {
            if self.0[index + 1..].contains(feature) {
                return Err(ValueError::FeatureListDuplicate(
                    feature.as_str().to_owned(),
                ));
            }
        }
        Ok(())
    }
}

impl Serialize for FeatureList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FeatureList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let list = Self(Vec::<FeatureId>::deserialize(deserializer)?);
        list.validate().map_err(DeError::custom)?;
        Ok(list)
    }
}

/// 无填充 base64url 的定长二进制字段；长度由解码后的字节数在校验期固定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base64Url<const BYTES: usize>([u8; BYTES]);

impl<const BYTES: usize> Base64Url<BYTES> {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let bytes = acpr_transcript::decode_base64url(text)
            .map_err(|_| ValueError::Base64Url(text.to_owned()))?;
        let array: [u8; BYTES] =
            bytes
                .try_into()
                .map_err(|bytes: Vec<u8>| ValueError::Base64UrlLength {
                    expected: BYTES,
                    actual: bytes.len(),
                })?;
        Ok(Self(array))
    }

    pub fn as_bytes(&self) -> &[u8; BYTES] {
        &self.0
    }
}

impl<const BYTES: usize> Serialize for Base64Url<BYTES> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&acpr_transcript::encode_base64url(&self.0))
    }
}

impl<'de, const BYTES: usize> Deserialize<'de> for Base64Url<BYTES> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// schema 里 `integer` + `minimum`/`maximum` 表达的有界整数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BoundedU64<const MIN: u64, const MAX: u64>(u64);

/// 只有下限的整数（schema 的 `minimum` 无 `maximum`）。
pub type UIntAtLeast<const MIN: u64> = BoundedU64<MIN, { u64::MAX }>;

/// v1 的信封 `protocolVersion`：`const 1`。
pub type ProtocolVersionV1 = BoundedU64<1, 1>;

impl<const MIN: u64, const MAX: u64> BoundedU64<MIN, MAX> {
    pub fn new(value: u64) -> Result<Self, ValueError> {
        if (MIN..=MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ValueError::IntegerRange {
                min: MIN,
                max: MAX,
                value,
            })
        }
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl<const MIN: u64, const MAX: u64> Serialize for BoundedU64<MIN, MAX> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.0)
    }
}

impl<'de, const MIN: u64, const MAX: u64> Deserialize<'de> for BoundedU64<MIN, MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(DeError::custom)
    }
}

/// schema 的 `maxLength` 文本（允许空串）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text<const MAX: usize>(String);

impl<const MAX: usize> Text<MAX> {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        if text.chars().count() > MAX {
            return Err(ValueError::TextLength {
                min: 0,
                max: MAX,
                actual: text.chars().count(),
            });
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<const MAX: usize> Serialize for Text<MAX> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for Text<MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// schema 的 `minLength: 1` + `maxLength` 文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonEmptyText<const MAX: usize>(String);

impl<const MAX: usize> NonEmptyText<MAX> {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let length = text.chars().count();
        if length == 0 || length > MAX {
            return Err(ValueError::TextLength {
                min: 1,
                max: MAX,
                actual: length,
            });
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<const MAX: usize> Serialize for NonEmptyText<MAX> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for NonEmptyText<MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// schema 里 `required` 且类型为 `T | null` 的字段：**键必须存在**，值可以是 `null`。
///
/// 缺键报错由 [`Deserialize`] 自己保证，不依赖调用方的中间形态：serde 处理缺失字段时会把
/// `serde::__private::de::missing_field` 的哨兵交给字段类型，该哨兵只在 `deserialize_option`
/// 上返回 `None`，在 `deserialize_any` 上是 missing-field 错误。本类型的 `Deserialize` 走
/// `deserialize_any`（缓冲成 `serde_json::Value` 再解出 `T`），因此：
///
/// - 缺键 → `missing field` 错误（这就是 schema `required` 的语义）；
/// - `null` → `Nullable::null()`；
/// - 其余 → `Nullable::value(T)`，`T` 自身的校验照常执行。
///
/// 想表达「键可以缺失」请用 `Option<Nullable<T>>`：外层 `Option` 在哨兵上返回 `None`，与
/// `Nullable` 在缺键时报错互不干扰（`Option` 判定的是"键在不在"，`Nullable` 判定的是
/// "值是不是 null"）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nullable<T>(Option<T>);

impl<T> Nullable<T> {
    pub fn null() -> Self {
        Self(None)
    }

    pub fn value(value: T) -> Self {
        Self(Some(value))
    }

    /// 从 `Option` 构造；**不**表示"键可以缺失"，缺键必须由调用方先行拒绝。
    pub fn from_option(value: Option<T>) -> Self {
        Self(value)
    }

    pub fn is_null(&self) -> bool {
        self.0.is_none()
    }

    pub fn as_ref(&self) -> Option<&T> {
        self.0.as_ref()
    }
}

impl<T: Serialize> Serialize for Nullable<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            Some(value) => value.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Nullable<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // `deserialize_any` 而不是 `deserialize_option`：后者会把缺失字段哨兵吃成 `None`，
        // 让 required 字段静默缺失（`error::Body::correlationId` 曾因此需要两段式反序列化）。
        let value = serde_json::Value::deserialize(deserializer)?;
        if value.is_null() {
            return Ok(Self::null());
        }
        serde_json::from_value::<T>(value)
            .map(Self::value)
            .map_err(DeError::custom)
    }
}

/// schema 里"键可缺失、但出现时不能为 `null`"的字段的 `deserialize_with`。
///
/// 配合 `#[serde(default, deserialize_with = "deserialize_optional_non_null")]` 使用：serde 的
/// `Option<T>` 会把 JSON `null` 与缺键一起收成 `None`，那等于接受 schema 没有的形状（这类字段的
/// schema 类型是数组或对象，没有 `null` 分支）。这里让"键存在性"由 serde 的 `default` 负责、"值非
/// `null`"由 `T::deserialize` 负责——显式 `null` 会以 serde 的类型错误被拒绝。
pub fn deserialize_optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// schema 的 `{"type": "object"}`：原样保留对象字节的开放扩展点（`error.body.details`）。
#[derive(Debug, Clone)]
pub struct RawObject(Box<serde_json::value::RawValue>);

impl PartialEq for RawObject {
    fn eq(&self, other: &Self) -> bool {
        self.0.get() == other.0.get()
    }
}

impl Eq for RawObject {}

impl RawObject {
    pub fn empty() -> Self {
        Self(serde_json::value::RawValue::from_string("{}".to_owned()).expect("{} 是合法 JSON"))
    }

    /// 从已经校验过的 JSON 文本构造；文本必须是单个 object。
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let raw = serde_json::value::RawValue::from_string(text.to_owned())
            .map_err(|_| ValueError::NotAnObject)?;
        if !raw.get().trim_start().starts_with('{') {
            return Err(ValueError::NotAnObject);
        }
        Ok(Self(raw))
    }

    pub fn as_raw(&self) -> &serde_json::value::RawValue {
        &self.0
    }

    pub fn get(&self) -> &str {
        self.0.get()
    }
}

impl Serialize for RawObject {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RawObject {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
        if !raw.get().trim_start().starts_with('{') {
            return Err(DeError::custom(ValueError::NotAnObject));
        }
        Ok(Self(raw))
    }
}

/// `timestamp`：`YYYY-MM-DDTHH:MM:SS.mmmZ`（`common.schema.json#/$defs/timestamp`）。
///
/// 判定分两步，与 fixture 侧的 ajv（`scripts/check-schema-fixtures.mjs` 打开
/// `validateFormats: true`，用 ajv-formats 的 `date-time`）一致：先按 schema 的 `pattern`
/// 校验逐位形状，再校验日历值与时刻范围（含闰年；闰秒只在 `23:59:60` 被接受）。
/// schema 的 `pattern` 已把时区固定为 `Z`、小数位固定为 3 位，这里不额外放宽。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(String);

impl Timestamp {
    /// 严格 24 字符：`\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z`。
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let bytes = text.as_bytes();
        let separators = [4_usize, 7, 10, 13, 16, 19, 23];
        let shaped = bytes.len() == 24
            && separators.iter().all(|index| {
                bytes[*index]
                    == match *index {
                        4 | 7 => b'-',
                        10 => b'T',
                        13 | 16 => b':',
                        19 => b'.',
                        _ => b'Z',
                    }
            })
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| separators.contains(&index) || byte.is_ascii_digit());
        if !shaped {
            return Err(ValueError::Timestamp(text.to_owned()));
        }

        let year = digits(&bytes[0..4]);
        let month = digits(&bytes[5..7]);
        let day = digits(&bytes[8..10]);
        let hour = digits(&bytes[11..13]);
        let minute = digits(&bytes[14..16]);
        let second = digits(&bytes[17..19]);
        let month_days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if is_leap_year(year) => 29,
            2 => 28,
            _ => 0,
        };
        let in_calendar = (1..=month_days).contains(&day)
            && hour <= 23
            && minute <= 59
            && (second <= 59 || (hour == 23 && minute == 59 && second == 60));
        if !in_calendar {
            return Err(ValueError::Timestamp(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `rawAcp.rawUnavailable.reason`（`common.schema.json#/$defs/rawAcp` 的 unavailable 分支）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawAcpUnavailableReason {
    SizeLimit,
    RetentionExpired,
    StorageFailure,
}

/// `common.schema.json#/$defs/rawAcp`：ACP 原文的两种互斥形状，`mediaType` 固定
/// `application/json`。
///
/// `Complete.raw_json` 用 [`RawValue`] 保留**字面量字节**（空白、键序与转义原样，不重新解析
/// 再序列化）；它必须是 JSON string 字面量（schema 的 `rawJson: {"type": "string"}`）。
/// `Unavailable.sha256` 是 required 且可 `null`。
#[derive(Debug, Clone)]
pub enum RawAcp {
    Complete {
        raw_json: Box<RawValue>,
        byte_length: DecimalString,
        sha256: Base64Url<32>,
    },
    Unavailable {
        reason: RawAcpUnavailableReason,
        byte_length: DecimalString,
        sha256: Nullable<Base64Url<32>>,
    },
}

/// `rawAcp.mediaType` 的 `const`。
const RAW_ACP_MEDIA_TYPE: &str = "application/json";

/// `rawAcp.rawUnavailable` 的对象形状（`additionalProperties: false`，三个键都必需）。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAcpUnavailableWire {
    reason: RawAcpUnavailableReason,
    #[serde(rename = "byteLength")]
    byte_length: DecimalString,
    sha256: Nullable<Base64Url<32>>,
}

impl PartialEq for RawAcp {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Complete {
                    raw_json: left_json,
                    byte_length: left_length,
                    sha256: left_hash,
                },
                Self::Complete {
                    raw_json: right_json,
                    byte_length: right_length,
                    sha256: right_hash,
                },
            ) => {
                left_json.get() == right_json.get()
                    && left_length == right_length
                    && left_hash == right_hash
            }
            (
                Self::Unavailable {
                    reason: left_reason,
                    byte_length: left_length,
                    sha256: left_hash,
                },
                Self::Unavailable {
                    reason: right_reason,
                    byte_length: right_length,
                    sha256: right_hash,
                },
            ) => {
                left_reason == right_reason
                    && left_length == right_length
                    && left_hash == right_hash
            }
            _ => false,
        }
    }
}

impl Eq for RawAcp {}

impl<'de> Deserialize<'de> for RawAcp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // 两种形状的字段集合不同（`additionalProperties: false`），且必须"要么完整、要么
        // unavailable"，所以逐个键收下再判定，而不是用 `#[serde(untagged)]`（untagged 会把值
        // 经 serde 的 `Content` 缓冲，`Box<RawValue>` 在缓冲路径上无法解出，见 §`ExtraFields`）。
        struct RawAcpVisitor;

        impl<'de> serde::de::Visitor<'de> for RawAcpVisitor {
            type Value = RawAcp;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("rawAcp 的 complete 或 unavailable 形状")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<Self::Value, A::Error> {
                let mut media_type: Option<String> = None;
                let mut raw_json: Option<Box<RawValue>> = None;
                let mut byte_length: Option<DecimalString> = None;
                let mut sha256: Option<Base64Url<32>> = None;
                let mut unavailable: Option<RawAcpUnavailableWire> = None;
                while let Some(key) = access.next_key::<String>()? {
                    match key.as_str() {
                        "mediaType" => media_type = Some(access.next_value()?),
                        "rawJson" => {
                            if raw_json.is_some() {
                                return Err(DeError::duplicate_field("rawJson"));
                            }
                            raw_json = Some(access.next_value()?);
                        }
                        "byteLength" => {
                            if byte_length.is_some() {
                                return Err(DeError::duplicate_field("byteLength"));
                            }
                            byte_length = Some(access.next_value()?);
                        }
                        "sha256" => {
                            if sha256.is_some() {
                                return Err(DeError::duplicate_field("sha256"));
                            }
                            sha256 = Some(access.next_value()?);
                        }
                        "rawUnavailable" => {
                            if unavailable.is_some() {
                                return Err(DeError::duplicate_field("rawUnavailable"));
                            }
                            unavailable = Some(access.next_value()?);
                        }
                        other => {
                            return Err(DeError::unknown_field(
                                other,
                                &[
                                    "mediaType",
                                    "rawJson",
                                    "byteLength",
                                    "sha256",
                                    "rawUnavailable",
                                ],
                            ));
                        }
                    }
                }

                match media_type.as_deref() {
                    Some(RAW_ACP_MEDIA_TYPE) => {}
                    Some(_) => {
                        return Err(DeError::custom(ValueError::Shape {
                            field: "rawAcp.mediaType",
                            expected: "\"application/json\"",
                        }));
                    }
                    None => return Err(DeError::missing_field("mediaType")),
                }

                match unavailable {
                    None => {
                        let raw_json = raw_json.ok_or_else(|| DeError::missing_field("rawJson"))?;
                        let byte_length =
                            byte_length.ok_or_else(|| DeError::missing_field("byteLength"))?;
                        let sha256 = sha256.ok_or_else(|| DeError::missing_field("sha256"))?;
                        if !raw_json.get().trim_start().starts_with('"') {
                            return Err(DeError::custom(ValueError::Shape {
                                field: "rawAcp.rawJson",
                                expected: "JSON string 字面量",
                            }));
                        }
                        Ok(RawAcp::Complete {
                            raw_json,
                            byte_length,
                            sha256,
                        })
                    }
                    Some(unavailable) => {
                        if raw_json.is_some() || byte_length.is_some() || sha256.is_some() {
                            return Err(DeError::custom(ValueError::Shape {
                                field: "rawAcp",
                                expected: "complete（rawJson/byteLength/sha256）与 unavailable 互斥",
                            }));
                        }
                        Ok(RawAcp::Unavailable {
                            reason: unavailable.reason,
                            byte_length: unavailable.byte_length,
                            sha256: unavailable.sha256,
                        })
                    }
                }
            }
        }

        deserializer.deserialize_map(RawAcpVisitor)
    }
}

/// `rawAcp.rawUnavailable` 的写出载体（字段名与 schema 逐字一致）。
#[derive(Serialize)]
struct RawAcpUnavailableJson<'a> {
    reason: RawAcpUnavailableReason,
    #[serde(rename = "byteLength")]
    byte_length: &'a DecimalString,
    sha256: &'a Nullable<Base64Url<32>>,
}

impl Serialize for RawAcp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // `rawJson` 是值槽上的 `Box<RawValue>`：serde_json 在自己的 `Serializer` 上按
        // `RawValue` 的私有 token 原样写字节。这条路径依赖"值槽的 serializer 就是 serde_json
        // 的 serializer"，因此 `RawAcp` 不能放进 `#[serde(flatten)]`（flatten 会把值经
        // serde 的 `Content` 缓冲，`RawValue` 在那里会退化成 `{"$serde_json::private::…"}`）。
        let mut map = serializer.serialize_map(Some(match self {
            Self::Complete { .. } => 4,
            Self::Unavailable { .. } => 2,
        }))?;
        map.serialize_entry("mediaType", RAW_ACP_MEDIA_TYPE)?;
        match self {
            Self::Complete {
                raw_json,
                byte_length,
                sha256,
            } => {
                map.serialize_entry("rawJson", raw_json)?;
                map.serialize_entry("byteLength", byte_length)?;
                map.serialize_entry("sha256", sha256)?;
            }
            Self::Unavailable {
                reason,
                byte_length,
                sha256,
            } => {
                map.serialize_entry(
                    "rawUnavailable",
                    &RawAcpUnavailableJson {
                        reason: *reason,
                        byte_length,
                        sha256,
                    },
                )?;
            }
        }
        map.end()
    }
}

/// `publicError`：跨消息引用的公开错误形状，四个键都必需。
///
/// 形状与校验（`deny_unknown_fields`、四个键全必需）是跨协议共用的，所以放这里；错误码的取值
/// 词表是协议语义（sync 线是 `ErrorCode`、node-link 线是它自己的错误码枚举），因此 `code` 的
/// 类型是类型参数 `Code`，由使用方绑定。
///
/// `details` 是 `{"type": "object"}` 的开放对象，按 [`RawObject`] 保留字节。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicError<Code> {
    pub code: Code,
    pub message: Text<1024>,
    pub retryable: bool,
    pub details: RawObject,
}

/// 开放对象（`additionalProperties: true` 的 view / 匿名扩展点）的未知字段载体，
/// 固定用法是 `#[serde(flatten)] pub extra: ExtraFields`；带它的 struct **不能**再加
/// `#[serde(deny_unknown_fields)]`（两者语义冲突，serde 会让未知字段直接报错而不是收进这里）。
///
/// 保真级别：
/// - 不经 flatten 的开放容器（`error.body.details`、`event.payload.view` 的 [`RawObject`]）是
///   **字节保真**：空白、键序、大整数字面量都原样。
/// - flatten 路径必然经过 serde 的 `Content` 缓冲，只保证**值保真**：未知键、嵌套结构、类型与
///   数字字面量（含超出 u64/i64 的大整数，靠 workspace 的 `serde_json/arbitrary_precision`）一致；
///   键序按 `BTreeMap` 升序、空白与浮点字面量写法会被规范化。
///
/// 值以 `Box<RawValue>` 存放（而不是 `serde_json::Value`）：消费者可以把它直接交给
/// `serde_json::from_str` 解成目标类型，不必先经过 `Value` 这层再转换。
/// 容器用 `BTreeMap` 而不是 `serde_json::Map`：后者的固有方法与 trait 实现只对
/// `Map<String, Value>` 提供（没有 `Default`/`Clone`/`iter`），放别的值类型无法使用。
///
/// `Serialize`/`Deserialize` 都是手写的：`Box<RawValue>` 的私有 token 路径在 flatten 的
/// `FlatMapSerializer`/`ContentDeserializer` 上都不可用（前者会把值写成
/// `{"$serde_json::private::RawValue": …}`，后者直接报 newtype 类型错），所以两个方向都经
/// `serde_json::Value` 中转。
#[derive(Debug, Clone, Default)]
pub struct ExtraFields(pub BTreeMap<String, Box<RawValue>>);

impl ExtraFields {
    /// 空的未知字段集合。
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 取某个未知字段的原始 JSON 文本。
    pub fn get(&self, key: &str) -> Option<&RawValue> {
        self.0.get(key).map(Box::as_ref)
    }

    /// 收下一个未知字段，返回被覆盖的旧值。
    pub fn insert(&mut self, key: String, value: Box<RawValue>) -> Option<Box<RawValue>> {
        self.0.insert(key, value)
    }

    /// 按 BTreeMap 的顺序遍历（键升序）。
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Box<RawValue>)> {
        self.0.iter()
    }
}

impl Serialize for ExtraFields {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, raw) in &self.0 {
            // 经 `serde_json::Value` 写出：flatten 的值槽是 serde 的 `FlatMapSerializer`，
            // 那里没有 `RawValue` 的 token 特例。
            let value: serde_json::Value =
                serde_json::from_str(raw.get()).map_err(serde::ser::Error::custom)?;
            map.serialize_entry(key, &value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for ExtraFields {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // flatten 已经把值缓冲成 serde 的 `Content`，`Box<RawValue>` 在那里解不出来；先把整个
        // 映射收成 `serde_json::Value`（它认得 `arbitrary_precision` 的数字 token），再逐个转成
        // `RawValue`。
        let buffered = BTreeMap::<String, serde_json::Value>::deserialize(deserializer)?;
        let mut extra = BTreeMap::new();
        for (key, value) in buffered {
            let raw = serde_json::value::to_raw_value(&value).map_err(DeError::custom)?;
            extra.insert(key, raw);
        }
        Ok(Self(extra))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个 `required` 且可 `null` 的字段。
    #[derive(Debug, Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct NullableHolder {
        #[serde(rename = "maybe")]
        maybe: Nullable<u32>,
    }

    /// 「键可以缺失」的写法：外层 `Option` 判定键在不在。
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct OptionalNullableHolder {
        #[serde(rename = "maybe")]
        maybe: Option<Nullable<u32>>,
    }

    #[test]
    fn nullable_field_requires_key() {
        let error = serde_json::from_str::<NullableHolder>("{}").expect_err("缺键必须报错");
        assert!(error.to_string().contains("maybe"), "{error}");

        // 想表达"键可以缺失"必须显式写 `Option<Nullable<T>>`；此时缺键仍是合法输入。
        let holder =
            serde_json::from_str::<OptionalNullableHolder>("{}").expect("外层 Option 允许缺键");
        assert!(holder.maybe.is_none());
    }

    #[test]
    fn nullable_field_accepts_null() {
        let holder =
            serde_json::from_str::<NullableHolder>(r#"{"maybe":null}"#).expect("null 是合法取值");
        assert!(holder.maybe.is_null());
        assert_eq!(holder.maybe.as_ref(), None);
        assert_eq!(
            serde_json::to_string(&holder).expect("可序列化"),
            r#"{"maybe":null}"#
        );
    }

    #[test]
    fn nullable_field_accepts_value() {
        let holder = serde_json::from_str::<NullableHolder>(r#"{"maybe":7}"#).expect("值本身合法");
        assert_eq!(holder.maybe.as_ref(), Some(&7));

        // `T` 自己的校验仍然生效：字符串不是 u32。
        assert!(serde_json::from_str::<NullableHolder>(r#"{"maybe":"7"}"#).is_err());
        assert!(serde_json::from_str::<NullableHolder>(r#"{"maybe":null,"extra":1}"#).is_err());
    }

    #[test]
    fn timestamp_accepts_canonical_instants_only() {
        for accepted in [
            "2026-09-17T12:10:00.123Z",
            "2026-09-17T00:00:00.000Z",
            "2028-02-29T23:59:59.999Z",
            // ajv-formats 允许的闰秒写法。
            "2026-06-30T23:59:60.000Z",
        ] {
            assert!(
                Timestamp::parse(accepted).is_ok(),
                "timestamp {accepted} 应被接受"
            );
        }

        for rejected in [
            "",
            "2026-09-17T12:10:00.123",
            "2026-09-17 12:10:00.123Z",
            "2026-09-17T12:10:00.12Z",
            "2026-09-17T12:10:00.1234Z",
            "2026-13-17T12:10:00.000Z",
            "2026-00-17T12:10:00.000Z",
            "2026-09-00T12:10:00.000Z",
            "2026-09-31T12:10:00.000Z",
            "2026-02-29T12:10:00.000Z",
            "2026-09-17T24:00:00.000Z",
            "2026-09-17T12:60:00.000Z",
            "2026-09-17T12:10:61.000Z",
            "2026-09-17T12:10:60.000Z",
            "2026-09-17T12:10:00.000z",
        ] {
            assert!(
                Timestamp::parse(rejected).is_err(),
                "timestamp {rejected} 应被拒绝"
            );
        }
    }

    /// flatten 路径的未知字段载体。
    #[derive(Debug, Deserialize, Serialize)]
    struct OpenObject {
        known: u32,
        #[serde(flatten)]
        extra: ExtraFields,
    }

    #[test]
    fn extra_fields_preserve_big_integers() {
        let text = r#"{"known":1,"big":123456789012345678901234567890,"nested":{"deep":-123456789012345678901234567890},"note":"x"}"#;
        let value: OpenObject = serde_json::from_str(text).expect("开放对象合法");
        assert_eq!(value.known, 1);
        assert_eq!(value.extra.len(), 3);
        assert!(value.extra.get("big").is_some());

        let encoded = serde_json::to_string(&value).expect("可序列化");
        for literal in [
            "123456789012345678901234567890",
            "-123456789012345678901234567890",
            r#""note":"x""#,
        ] {
            assert!(
                encoded.contains(literal),
                "{literal} 未逐字符保留：{encoded}"
            );
        }
    }
}
