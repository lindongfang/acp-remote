//! capability 的 wire 形状与「宣告必须真实」的落点。
//!
//! 两条不变量（`docs/AGENTS.md` §3、`docs/ACP_COMPATIBILITY_MATRIX.md` §3.3/§5 第 6–7 条）：
//!
//! 1. **保真**：解码时未识别的能力字段必须保留（`_meta` 与其它未知键都进 `extra`/`meta`）；
//! 2. **真实**：本 crate 只能宣告自己确实具备的能力——[`ClientCapabilities::none`] 序列化出来是
//!    空对象，不会用默认值、空对象或占位符把未实现的可选能力表达成「已支持」。
//!
//! 需要宣告某项能力时显式构造对应字段（由组合根决定），而不是靠 serde 默认值。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `InitializeRequest.protocolVersion` 的 v1 取值。
pub const PROTOCOL_VERSION_V1: u32 = 1;

/// 客户端能力声明。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClientCapabilities {
    /// 文件系统能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fs: Option<FileSystemCapabilities>,
    /// 是否支持终端服务方法。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<bool>,
    /// 会话配置选项能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<ClientSessionCapabilities>,
    /// 认证能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<ClientAuthCapabilities>,
    /// elicitation 能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<ElicitationCapabilities>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别的能力字段，原样保留。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ClientCapabilities {
    /// **不宣告任何可选能力**。
    ///
    /// 这是本阶段的默认值：`agent-host` 还没有可供用户作答的上游客户端，因此不得宣告 elicitation、
    /// fs、terminal 等可选能力。序列化结果是空对象，实现侧无法靠「默认值」把它伪装成已支持。
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// 是否宣告了 elicitation form 能力。
    #[must_use]
    pub fn supports_elicitation_form(&self) -> bool {
        self.elicitation
            .as_ref()
            .is_some_and(|caps| caps.form.is_some())
    }
}

/// 文件系统能力。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FileSystemCapabilities {
    /// 读取文本文件。
    #[serde(
        rename = "readTextFile",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub read_text_file: Option<bool>,
    /// 写入文本文件。
    #[serde(
        rename = "writeTextFile",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub write_text_file: Option<bool>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 会话相关客户端能力。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClientSessionCapabilities {
    /// 配置选项能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_options: Option<SessionConfigOptionsCapabilities>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 配置选项能力。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SessionConfigOptionsCapabilities {
    /// 布尔型配置选项。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boolean: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 客户端认证能力。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClientAuthCapabilities {
    /// 终端型认证。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<bool>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// elicitation 能力。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ElicitationCapabilities {
    /// form 模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<Value>,
    /// url 模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Agent 能力声明。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AgentCapabilities {
    /// 是否支持 `session/load`。
    #[serde(
        rename = "loadSession",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub load_session: Option<bool>,
    /// prompt 内容能力。
    #[serde(
        rename = "promptCapabilities",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_capabilities: Option<PromptCapabilities>,
    /// MCP 能力。
    #[serde(
        rename = "mcpCapabilities",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub mcp_capabilities: Option<McpCapabilities>,
    /// 会话方法能力。
    #[serde(
        rename = "sessionCapabilities",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub session_capabilities: Option<SessionCapabilities>,
    /// 认证能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AgentAuthCapabilities>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段，原样保留。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl AgentCapabilities {
    /// 是否宣告支持 `session/load`。
    #[must_use]
    pub fn supports_load_session(&self) -> bool {
        self.load_session == Some(true)
    }

    /// 是否宣告支持图片 prompt 输入。
    #[must_use]
    pub fn supports_prompt_image(&self) -> bool {
        self.prompt_capabilities
            .as_ref()
            .is_some_and(|caps| caps.image == Some(true))
    }

    /// 是否宣告支持音频 prompt 输入。
    #[must_use]
    pub fn supports_prompt_audio(&self) -> bool {
        self.prompt_capabilities
            .as_ref()
            .is_some_and(|caps| caps.audio == Some(true))
    }

    /// 是否宣告支持内嵌上下文（`resource` 形式）。
    #[must_use]
    pub fn supports_embedded_context(&self) -> bool {
        self.prompt_capabilities
            .as_ref()
            .is_some_and(|caps| caps.embedded_context == Some(true))
    }

    /// 是否宣告支持 `session/list`。
    #[must_use]
    pub fn supports_session_list(&self) -> bool {
        self.session_capabilities
            .as_ref()
            .is_some_and(|caps| caps.list.is_some())
    }

    /// 是否宣告支持 `session/resume`。
    #[must_use]
    pub fn supports_session_resume(&self) -> bool {
        self.session_capabilities
            .as_ref()
            .is_some_and(|caps| caps.resume.is_some())
    }

    /// 是否宣告支持 `session/close`。
    #[must_use]
    pub fn supports_session_close(&self) -> bool {
        self.session_capabilities
            .as_ref()
            .is_some_and(|caps| caps.close.is_some())
    }

    /// 是否宣告支持 `session/delete`。
    #[must_use]
    pub fn supports_session_delete(&self) -> bool {
        self.session_capabilities
            .as_ref()
            .is_some_and(|caps| caps.delete.is_some())
    }
}

/// prompt 内容能力。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PromptCapabilities {
    /// 图片。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<bool>,
    /// 音频。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<bool>,
    /// 内嵌上下文。
    #[serde(
        rename = "embeddedContext",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub embedded_context: Option<bool>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// MCP 能力。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct McpCapabilities {
    /// HTTP transport。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<bool>,
    /// SSE transport。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sse: Option<bool>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 会话方法能力：字段**存在**即表示支持（上游用空对象表达）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SessionCapabilities {
    /// `session/list`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<Value>,
    /// `session/delete`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete: Option<Value>,
    /// additionalDirectories。
    #[serde(
        rename = "additionalDirectories",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_directories: Option<Value>,
    /// `session/resume`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<Value>,
    /// `session/close`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Agent 认证能力。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AgentAuthCapabilities {
    /// `logout` 方法。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logout: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// 实现信息（`clientInfo` / `agentInfo`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Implementation {
    /// 名称。
    pub name: String,
    /// 版本。
    pub version: String,
    /// 可选标题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// 未识别字段。
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
