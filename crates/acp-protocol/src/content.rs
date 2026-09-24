//! 结构化内容：content block、tool call content、diff 与 terminal。
//!
//! 这些类型是「结构化内容不得被文本化」的落点（`AGENTS.md` §3、`docs/ACP_COMPATIBILITY_MATRIX.md`
//! §5 第 4 条）：tool call、diff、terminal 与 resource 都必须按结构化类型解码，不得合并成普通文本。
//!
//! 未知的 content block 类型**不是**解码失败，而是 [`ContentBlock::Unknown`]（可见降级）：原文在
//! `RawDocument` 里逐字节保留，类型化视图也保留整个 payload，因此上层可以显式降级而不是丢内容。

use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

/// ACP 的五种 content block 加一个可见降级分支。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// 文本。
    Text(TextContent),
    /// 图片（base64）。
    Image(ImageContent),
    /// 音频（base64）。
    Audio(AudioContent),
    /// 资源链接。
    ResourceLink(ResourceLink),
    /// 内嵌资源。
    Resource(EmbeddedResource),
    /// 未登记的 content block 类型：可见降级，原文与整个 payload 都保留。
    ///
    /// 它**不参与序列化**（保真路径是 `RawDocument`，不是把类型化视图写回去）；需要原样输出时用
    /// [`ContentBlock::unknown_payload`]。
    #[serde(skip_serializing)]
    Unknown {
        /// 未登记的类型判别子。
        wire_type: String,
        /// 整个 content block 的原始 payload。
        payload: Value,
    },
}

impl<'de> Deserialize<'de> for ContentBlock {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let wire_type = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| de::Error::missing_field("type"))?
            .to_owned();
        match wire_type.as_str() {
            "text" => from_value(value).map(ContentBlock::Text),
            "image" => from_value(value).map(ContentBlock::Image),
            "audio" => from_value(value).map(ContentBlock::Audio),
            "resource_link" => from_value(value).map(ContentBlock::ResourceLink),
            "resource" => from_value(value).map(ContentBlock::Resource),
            _ => Ok(ContentBlock::Unknown {
                wire_type,
                payload: value,
            }),
        }
    }
}

fn from_value<T, E>(value: Value) -> Result<T, E>
where
    T: serde::de::DeserializeOwned,
    E: de::Error,
{
    serde_json::from_value(value).map_err(de::Error::custom)
}

impl ContentBlock {
    /// 文本内容（仅 [`ContentBlock::Text`]）。用于把流式文本投影成 delta 视图。
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(content) => Some(&content.text),
            _ => None,
        }
    }

    /// 未登记类型时返回 `(判别子, payload)`，供上层显式降级。
    #[must_use]
    pub fn unknown_payload(&self) -> Option<(&str, &Value)> {
        match self {
            Self::Unknown { wire_type, payload } => Some((wire_type, payload)),
            _ => None,
        }
    }

    /// 判别子（与线上 `type` 字段一致）。
    #[must_use]
    pub fn wire_type(&self) -> &str {
        match self {
            Self::Text(_) => "text",
            Self::Image(_) => "image",
            Self::Audio(_) => "audio",
            Self::ResourceLink(_) => "resource_link",
            Self::Resource(_) => "resource",
            Self::Unknown { wire_type, .. } => wire_type,
        }
    }

    /// 是否文本块。
    #[must_use]
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }
}

/// `text` content block。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextContent {
    /// 文本。
    pub text: String,
    /// 可选注解（开放对象，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
    /// `_meta`（开放对象，原样保留）。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `image` content block。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageContent {
    /// base64 数据（wire 上是字符串，本 crate 不解码，避免破坏原文）。
    pub data: String,
    /// MIME 类型。
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// 可选 URI。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// 可选注解。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `audio` content block。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioContent {
    /// base64 数据。
    pub data: String,
    /// MIME 类型。
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// 可选注解。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `resource_link` content block。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceLink {
    /// 名称。
    pub name: String,
    /// URI。
    pub uri: String,
    /// 可选描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 可选 MIME 类型。
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// 可选大小。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    /// 可选标题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 可选注解。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `resource` content block（内嵌资源）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedResource {
    /// 资源内容（text 或 blob 两种之一，未知字段原样保留）。
    pub resource: EmbeddedResourceContents,
    /// 可选注解。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 内嵌资源的内容：`text` 或 `blob`，其余字段原样保留。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedResourceContents {
    /// 资源 URI。
    pub uri: String,
    /// 可选 MIME 类型。
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// 文本内容（`text` 形式）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// 二进制内容（`blob` 形式，base64 字符串原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
    /// 其它字段（含 `_meta`）原样保留。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// tool call 里的 diff 内容块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diff {
    /// 被修改的路径（Agent 报什么就是什么，本 crate 不解析、不规范化）。
    pub path: String,
    /// 修改前内容。
    #[serde(rename = "oldText", default, skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    /// 修改后内容。
    #[serde(rename = "newText")]
    pub new_text: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// tool call 里的 terminal 引用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Terminal {
    /// 终端标识。
    #[serde(rename = "terminalId")]
    pub terminal_id: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// tool call 的 `content` 数组元素：`content` | `diff` | `terminal`。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCallContent {
    /// 普通内容块。
    Content(ContentWrapper),
    /// diff。
    Diff(Diff),
    /// 终端引用。
    Terminal(Terminal),
    /// 未登记的判别子：可见降级，payload 原样保留，不参与序列化。
    #[serde(skip_serializing)]
    Unknown {
        /// 未登记的判别子。
        wire_type: String,
        /// 整个元素。
        payload: Value,
    },
}

impl<'de> Deserialize<'de> for ToolCallContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let wire_type = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| de::Error::missing_field("type"))?
            .to_owned();
        match wire_type.as_str() {
            "content" => from_value(value).map(ToolCallContent::Content),
            "diff" => from_value(value).map(ToolCallContent::Diff),
            "terminal" => from_value(value).map(ToolCallContent::Terminal),
            _ => Ok(ToolCallContent::Unknown {
                wire_type,
                payload: value,
            }),
        }
    }
}

impl ToolCallContent {
    /// 判别子。
    #[must_use]
    pub fn wire_type(&self) -> &str {
        match self {
            Self::Content(_) => "content",
            Self::Diff(_) => "diff",
            Self::Terminal(_) => "terminal",
            Self::Unknown { wire_type, .. } => wire_type,
        }
    }
}

/// `content` 形式的 tool call content（包一层 content block）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentWrapper {
    /// 内层 content block。
    pub content: ContentBlock,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// tool call 的位置（`line` 是上游快照的 `uint32`，因此用 `u32` 承载以拒绝负值与溢出）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallLocation {
    /// 路径。
    pub path: String,
    /// 行号；`u32` 而非 `i64` 是刻意的：上游 `$defs/ToolCallLocation.line` 声明 `format: uint32`，
    /// 负值必须在解码边界被拒（`fixtures/acp/v1/invalid/tool-call-location-negative-line.json`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}
