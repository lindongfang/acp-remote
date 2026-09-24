//! 类型化消息：我们**发送**的请求、我们**读取**的响应，以及 Agent 发来的请求与通知。
//!
//! 解码入口分成两侧，各自校验方向：本机作为 ACP Client 发送 client→agent 的方法、读取它们的响应；
//! 接收 agent→client 的请求与通知。方向不符是 [`AcpError::WrongDirection`]，方法未实现是
//! [`AcpError::Unsupported`]（显式不支持）。
//!
//! 编码入口（[`request`]、[`notification`]、[`response`]、[`error_response`]）产出的仍然是
//! [`RawDocument`]：发出去的消息也要经过同一套长度上限与结构校验，且调用方拿到的是原文，
//! 便于日志与测试逐字节比对。

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::capability::{
    AgentCapabilities, ClientCapabilities, Implementation, PROTOCOL_VERSION_V1,
};
use crate::content::ContentBlock;
use crate::envelope::{Envelope, MessageClass};
use crate::error::{AcpError, Result};
use crate::methods::MethodDirection;
use crate::raw::{IdLiteral, RawDocument};
use crate::update::{SessionUpdate, ToolCallUpdate};

/// JSON-RPC：方法未找到。
pub const CODE_METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC：参数不合法。
pub const CODE_INVALID_PARAMS: i64 = -32602;
/// JSON-RPC：内部错误。
pub const CODE_INTERNAL_ERROR: i64 = -32603;

// -------------------------------------------------------------------------------------------
// 编码：我们发出去的消息
// -------------------------------------------------------------------------------------------

/// 请求（有 `id`）。
pub fn request(id_literal: &str, method: &str, params: &Value) -> Result<RawDocument> {
    // 校验 id 字面量合法，避免把不合法的 id 发出去。
    IdLiteral::from_literal(id_literal)?;
    let document = json!({
        "jsonrpc": "2.0",
        "id": serde_json::from_str::<Value>(id_literal).map_err(|error| AcpError::InvalidField {
            field: "id".to_owned(),
            detail: error.to_string(),
        })?,
        "method": method,
        "params": params,
    });
    RawDocument::parse(document.to_string())
}

/// 以整数 `id` 发出的请求（`agent-host` 的请求 id 由它自己单调分配）。
pub fn request_with_u64_id(id: u64, method: &str, params: &Value) -> Result<RawDocument> {
    request(&id.to_string(), method, params)
}

/// 通知（没有 `id`）。
pub fn notification(method: &str, params: &Value) -> Result<RawDocument> {
    let document = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    RawDocument::parse(document.to_string())
}

/// 成功响应。
pub fn response(id: &IdLiteral, result: &Value) -> Result<RawDocument> {
    let document = json!({
        "jsonrpc": "2.0",
        "id": id_value(id)?,
        "result": result,
    });
    RawDocument::parse(document.to_string())
}

/// 错误响应。`message` 只应包含协议元数据（方法名、字段名等），不得包含消息正文或凭据。
pub fn error_response(id: &IdLiteral, code: i64, message: &str) -> Result<RawDocument> {
    let document = json!({
        "jsonrpc": "2.0",
        "id": id_value(id)?,
        "error": { "code": code, "message": message },
    });
    RawDocument::parse(document.to_string())
}

fn id_value(id: &IdLiteral) -> Result<Value> {
    serde_json::from_str(id.as_json()).map_err(|error| AcpError::InvalidField {
        field: "id".to_owned(),
        detail: error.to_string(),
    })
}

// -------------------------------------------------------------------------------------------
// 解码：我们读取的响应（没有方法名，方向由发起方决定）
// -------------------------------------------------------------------------------------------

/// 把一个响应信封解码成类型化值。
pub fn decode_response<T>(envelope: &Envelope) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    if envelope.class() != MessageClass::Response {
        return Err(AcpError::Malformed {
            detail: "不是响应".to_owned(),
        });
    }
    let result = envelope.result()?;
    serde_json::from_value(result).map_err(|error| AcpError::InvalidField {
        field: "result".to_owned(),
        detail: error.to_string(),
    })
}

// -------------------------------------------------------------------------------------------
// 解码：Agent 发来的请求与通知
// -------------------------------------------------------------------------------------------

/// 校验信封是已实现的 agent→client 方法，并返回其 `params` 对象。
pub fn agent_params(envelope: &Envelope, method: &str) -> Result<Value> {
    let actual = envelope.ensure_direction(MethodDirection::AgentToClient)?;
    if actual != method {
        return Err(AcpError::InvalidField {
            field: "method".to_owned(),
            detail: format!("期望 {method}，收到 {actual}"),
        });
    }
    envelope.params()
}

/// 校验信封是已实现的 client→agent 方法，并返回其 `params` 对象。
pub fn client_params(envelope: &Envelope, method: &str) -> Result<Value> {
    let actual = envelope.ensure_direction(MethodDirection::ClientToAgent)?;
    if actual != method {
        return Err(AcpError::InvalidField {
            field: "method".to_owned(),
            detail: format!("期望 {method}，收到 {actual}"),
        });
    }
    envelope.params()
}

/// `session/update` 通知。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionNotification {
    /// 该更新所属的 ACP 会话。
    pub session_id: String,
    /// 更新内容（含未知判别子的可见降级）。
    pub update: SessionUpdate,
    /// `_meta`。
    pub meta: Option<Value>,
}

impl SessionNotification {
    /// 从 `session/update` 的 `params` 解码。
    pub fn from_params(params: &Value) -> Result<Self> {
        let session_id = params
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| AcpError::MissingField {
                field: "sessionId".to_owned(),
            })?
            .to_owned();
        let update = params.get("update").ok_or_else(|| AcpError::MissingField {
            field: "update".to_owned(),
        })?;
        Ok(Self {
            session_id,
            update: SessionUpdate::decode(update)?,
            meta: params.get("_meta").cloned(),
        })
    }
}

/// `session/update` 通知（含方向校验）。
pub fn session_notification(envelope: &Envelope) -> Result<SessionNotification> {
    SessionNotification::from_params(&agent_params(envelope, "session/update")?)
}

/// Agent 发来的权限请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestPermissionRequest {
    /// 会话标识。
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// 触发该请求的 tool call。
    #[serde(rename = "toolCall")]
    pub tool_call: ToolCallUpdate,
    /// 可选项。
    pub options: Vec<PermissionOption>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 一个权限选项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionOption {
    /// 选项标识（回传时必须原样带上）。
    #[serde(rename = "optionId")]
    pub option_id: String,
    /// 展示名。
    pub name: String,
    /// 类别（开放取值）。
    pub kind: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 权限解析结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PermissionOutcome {
    /// 用户选择了一个选项。
    Selected {
        /// 选项标识（原样回传 Agent 给的值）。
        #[serde(rename = "optionId")]
        option_id: String,
    },
    /// turn 在用户作答前被取消。
    Cancelled,
}

/// 权限请求的响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestPermissionResponse {
    /// 结果。
    pub outcome: PermissionOutcome,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// elicitation 的两种模式。
#[derive(Debug, Clone, PartialEq)]
pub enum ElicitationMode {
    /// 表单模式：`requestedSchema` 按原始对象保留（结构较深，本 crate 不重复建模）。
    Form {
        /// 请求的表单 schema。
        requested_schema: Value,
        /// 可选的会话作用域。
        session_scope: Option<Value>,
    },
    /// URL 模式：`elicitationId` + `url`。
    Url {
        /// elicitation 标识。
        elicitation_id: String,
        /// URL。
        url: String,
    },
    /// 未识别的模式：可见降级，payload 原样保留。
    Unknown {
        /// 整个 params。
        params: Value,
    },
}

/// Agent 发来的 elicitation 请求。
#[derive(Debug, Clone, PartialEq)]
pub struct ElicitationRequest {
    /// 面向用户的说明。
    pub message: String,
    /// 模式。
    pub mode: ElicitationMode,
    /// `_meta`。
    pub meta: Option<Value>,
}

impl ElicitationRequest {
    /// 从 `elicitation/create` 的 `params` 解码。
    pub fn from_params(params: &Value) -> Result<Self> {
        let message = params
            .get("message")
            .and_then(Value::as_str)
            .ok_or_else(|| AcpError::MissingField {
                field: "message".to_owned(),
            })?
            .to_owned();
        let mode = match params.get("mode").and_then(Value::as_str) {
            Some("form") => ElicitationMode::Form {
                requested_schema: params
                    .get("requestedSchema")
                    .ok_or_else(|| AcpError::MissingField {
                        field: "requestedSchema".to_owned(),
                    })?
                    .clone(),
                session_scope: params.get("session").cloned(),
            },
            Some("url") => ElicitationMode::Url {
                elicitation_id: params
                    .get("elicitationId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AcpError::MissingField {
                        field: "elicitationId".to_owned(),
                    })?
                    .to_owned(),
                url: params
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AcpError::MissingField {
                        field: "url".to_owned(),
                    })?
                    .to_owned(),
            },
            _ => ElicitationMode::Unknown {
                params: params.clone(),
            },
        };
        Ok(Self {
            message,
            mode,
            meta: params.get("_meta").cloned(),
        })
    }
}

/// elicitation 的作答动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElicitationAction {
    /// 接受并提交内容。
    Accept,
    /// 拒绝。
    Decline,
    /// 取消。
    Cancel,
}

impl ElicitationAction {
    /// 线上的 `action` 取值。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Cancel => "cancel",
        }
    }
}

/// elicitation 请求的响应。
#[derive(Debug, Clone, PartialEq)]
pub struct ElicitationResponse {
    /// 动作。
    pub action: ElicitationAction,
    /// 接受时提交的内容（`null` 与省略等价）。
    pub content: Option<Value>,
}

impl ElicitationResponse {
    /// 序列化为响应 `result`。
    #[must_use]
    pub fn to_result(&self) -> Value {
        let mut result = json!({ "action": self.action.as_str() });
        if let Some(content) = &self.content {
            if let Some(object) = result.as_object_mut() {
                object.insert("content".to_owned(), content.clone());
            }
        }
        result
    }
}

// -------------------------------------------------------------------------------------------
// 我们发送的请求体与读取的响应体
// -------------------------------------------------------------------------------------------

/// `initialize` 请求参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitializeRequest {
    /// 协议版本（v1 = 1）。
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u32,
    /// 客户端能力声明。**只能宣告确实具备的能力**（见 [`ClientCapabilities::none`]）。
    #[serde(rename = "clientCapabilities")]
    pub client_capabilities: ClientCapabilities,
    /// 客户端信息。
    #[serde(
        rename = "clientInfo",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub client_info: Option<Implementation>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl InitializeRequest {
    /// 以 v1 协议版本与给定的能力声明构造。
    #[must_use]
    pub fn new(client_capabilities: ClientCapabilities) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION_V1,
            client_capabilities,
            client_info: None,
            meta: None,
        }
    }
}

/// `initialize` 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitializeResponse {
    /// 协议版本。
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u32,
    /// Agent 能力声明（未识别的能力字段保留在 `extra`）。
    #[serde(
        rename = "agentCapabilities",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub agent_capabilities: Option<AgentCapabilities>,
    /// 认证方法。
    #[serde(
        rename = "authMethods",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub auth_methods: Option<Vec<Value>>,
    /// Agent 信息。
    #[serde(rename = "agentInfo", default, skip_serializing_if = "Option::is_none")]
    pub agent_info: Option<Implementation>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl InitializeResponse {
    /// Agent 能力；未宣告时返回默认（全部未支持）。
    #[must_use]
    pub fn capabilities(&self) -> AgentCapabilities {
        self.agent_capabilities.clone().unwrap_or_default()
    }
}

/// `session/new` 请求参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewSessionRequest {
    /// 工作目录（**已由 core 解析的规范化绝对路径**，见 `workspace-resolution` 规范）。
    pub cwd: String,
    /// MCP server 列表；首阶段固定为空数组。
    #[serde(rename = "mcpServers")]
    pub mcp_servers: Vec<Value>,
    /// 额外目录（首阶段不发送）。
    #[serde(
        rename = "additionalDirectories",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_directories: Option<Vec<String>>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl NewSessionRequest {
    /// 以工作目录构造（`mcpServers` 为空数组）。
    #[must_use]
    pub fn new(cwd: impl Into<String>) -> Self {
        Self {
            cwd: cwd.into(),
            mcp_servers: Vec::new(),
            additional_directories: None,
            meta: None,
        }
    }
}

/// `session/new` 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewSessionResponse {
    /// ACP 会话标识。
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// 模式状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modes: Option<SessionModeState>,
    /// 配置选项（结构较深，按原始对象保留）。
    #[serde(
        rename = "configOptions",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub config_options: Option<Vec<Value>>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 模式状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionModeState {
    /// 当前模式。
    #[serde(rename = "currentModeId")]
    pub current_mode_id: String,
    /// 可用模式。
    #[serde(rename = "availableModes")]
    pub available_modes: Vec<SessionMode>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 一个模式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMode {
    /// 模式标识。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `session/prompt` 请求参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptRequest {
    /// 会话标识。
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// prompt 内容块。
    pub prompt: Vec<ContentBlock>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `session/prompt` 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptResponse {
    /// 停止原因（`end_turn` / `max_tokens` / `max_turn_requests` / `refusal` / `cancelled`）。
    #[serde(rename = "stopReason")]
    pub stop_reason: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl PromptResponse {
    /// 是否是客户端取消导致的停止。
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.stop_reason == "cancelled"
    }
}

/// `session/cancel` 通知参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelNotification {
    /// 会话标识。
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `session/set_mode` 请求参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetSessionModeRequest {
    /// 会话标识。
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// 目标模式。
    #[serde(rename = "modeId")]
    pub mode_id: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `session/set_config_option` 请求参数。
///
/// `value` 在协议里是封闭联合（布尔或 value-id 字符串），本 crate 用 [`config_value_boolean`] 与
/// [`config_value_id`] 构造，避免调用方手写形状出错。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetSessionConfigOptionRequest {
    /// 会话标识。
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// 配置项标识。
    #[serde(rename = "configId")]
    pub config_id: String,
    /// 新取值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 构造布尔型配置取值。
#[must_use]
pub fn config_value_boolean(value: bool) -> Value {
    json!({ "type": "boolean", "value": value })
}

/// 构造 value-id 型配置取值。
#[must_use]
pub fn config_value_id(value_id: &str) -> Value {
    json!({ "value": value_id })
}
