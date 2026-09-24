//! `session/update` 的 11 种已知判别子与可见降级。
//!
//! `session/update` 的 `update` 对象由 `sessionUpdate` 判别。矩阵登记了 11 种（`docs/ACP_COMPATIBILITY_MATRIX.md`
//! §3.2 的 `sessionUpdates` 组，与 [`crate::methods::SESSION_UPDATE_WIRE_VALUES`] 一一对应）。
//!
//! 未登记的判别子**不是错误**，而是 [`SessionUpdate::Unknown`]（`invariant.future_update_visible` 的
//! `visible_degradation`）：整个 payload 原样保留，上层必须把原始 document 一并交给 core，
//! 既不能静默丢弃，也不能降格成普通文本。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::content::{ContentBlock, ToolCallContent, ToolCallLocation};
use crate::error::{AcpError, Result};

/// `session/update` 的一种更新。
#[derive(Debug, Clone, PartialEq)]
pub enum SessionUpdate {
    /// 用户消息的流式片段。
    UserMessageChunk(ContentChunk),
    /// Agent 回复的流式片段。
    AgentMessageChunk(ContentChunk),
    /// Agent 思考过程的流式片段。
    AgentThoughtChunk(ContentChunk),
    /// 新的 tool call。
    ToolCall(ToolCall),
    /// tool call 的状态或结果更新。
    ToolCallUpdate(ToolCallUpdate),
    /// 执行计划。
    Plan(Plan),
    /// 可用命令的变化。
    AvailableCommandsUpdate(AvailableCommandsUpdate),
    /// 当前模式变化。
    CurrentModeUpdate(CurrentModeUpdate),
    /// 配置选项变化。
    ConfigOptionUpdate(ConfigOptionUpdate),
    /// 会话元信息变化。
    SessionInfoUpdate(SessionInfoUpdate),
    /// 上下文窗口与成本更新。
    UsageUpdate(UsageUpdate),
    /// 未登记的判别子：可见降级，payload 原样保留。
    Unknown {
        /// 未登记的判别子。
        wire_value: String,
        /// 整个 `update` 对象。
        payload: Value,
    },
}

impl SessionUpdate {
    /// 解码一个 `update` 对象。
    ///
    /// 缺少 `sessionUpdate` 或已知判别子缺 required 字段时返回错误；未登记判别子返回
    /// [`SessionUpdate::Unknown`]。
    pub fn decode(payload: &Value) -> Result<Self> {
        let wire_value = payload
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .ok_or_else(|| AcpError::MissingField {
                field: "sessionUpdate".to_owned(),
            })?;
        match wire_value {
            "user_message_chunk" => parse(payload).map(Self::UserMessageChunk),
            "agent_message_chunk" => parse(payload).map(Self::AgentMessageChunk),
            "agent_thought_chunk" => parse(payload).map(Self::AgentThoughtChunk),
            "tool_call" => parse(payload).map(Self::ToolCall),
            "tool_call_update" => parse(payload).map(Self::ToolCallUpdate),
            "plan" => parse(payload).map(Self::Plan),
            "available_commands_update" => parse(payload).map(Self::AvailableCommandsUpdate),
            "current_mode_update" => parse(payload).map(Self::CurrentModeUpdate),
            "config_option_update" => parse(payload).map(Self::ConfigOptionUpdate),
            "session_info_update" => parse(payload).map(Self::SessionInfoUpdate),
            "usage_update" => parse(payload).map(Self::UsageUpdate),
            other => Ok(Self::Unknown {
                wire_value: other.to_owned(),
                payload: payload.clone(),
            }),
        }
    }

    /// 判别子（与线上 `sessionUpdate` 字段一致）。
    #[must_use]
    pub fn wire_value(&self) -> &str {
        match self {
            Self::UserMessageChunk(_) => "user_message_chunk",
            Self::AgentMessageChunk(_) => "agent_message_chunk",
            Self::AgentThoughtChunk(_) => "agent_thought_chunk",
            Self::ToolCall(_) => "tool_call",
            Self::ToolCallUpdate(_) => "tool_call_update",
            Self::Plan(_) => "plan",
            Self::AvailableCommandsUpdate(_) => "available_commands_update",
            Self::CurrentModeUpdate(_) => "current_mode_update",
            Self::ConfigOptionUpdate(_) => "config_option_update",
            Self::SessionInfoUpdate(_) => "session_info_update",
            Self::UsageUpdate(_) => "usage_update",
            Self::Unknown { wire_value, .. } => wire_value,
        }
    }

    /// 是否可见降级的未知更新。
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown { .. })
    }

    /// 流式文本片段的内容块（三个 `*_chunk` 变体）。
    #[must_use]
    pub fn chunk(&self) -> Option<&ContentChunk> {
        match self {
            Self::UserMessageChunk(chunk)
            | Self::AgentMessageChunk(chunk)
            | Self::AgentThoughtChunk(chunk) => Some(chunk),
            _ => None,
        }
    }
}

fn parse<T>(payload: &Value) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(payload.clone()).map_err(|error| AcpError::InvalidField {
        field: "update".to_owned(),
        detail: error.to_string(),
    })
}

/// 流式内容片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentChunk {
    /// 内容块。
    pub content: ContentBlock,
    /// 可选消息标识（用于把片段归并到同一条消息）。
    #[serde(rename = "messageId", default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 新的 tool call。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// tool call 标识。
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// 标题。
    pub title: String,
    /// 工具名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 工具类别（开放取值，未知值原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 状态（开放取值）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 内容块数组（diff/terminal/内容块保持结构）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ToolCallContent>>,
    /// 位置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<ToolCallLocation>>,
    /// 原始输入（ACP 扩展容器，原样保留）。
    #[serde(rename = "rawInput", default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<Value>,
    /// 原始输出（ACP 扩展容器，原样保留）。
    #[serde(rename = "rawOutput", default, skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// tool call 的增量更新。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallUpdate {
    /// tool call 标识。
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// 工具类别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 标题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 工具名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 内容块数组。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ToolCallContent>>,
    /// 位置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<ToolCallLocation>>,
    /// 原始输入。
    #[serde(rename = "rawInput", default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<Value>,
    /// 原始输出。
    #[serde(rename = "rawOutput", default, skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 执行计划。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// 计划条目。
    pub entries: Vec<PlanEntry>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 一条计划条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanEntry {
    /// 条目内容。
    pub content: String,
    /// 优先级（开放取值）。
    pub priority: String,
    /// 状态（开放取值）。
    pub status: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 可用命令更新。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvailableCommandsUpdate {
    /// 可用命令。
    #[serde(rename = "availableCommands")]
    pub available_commands: Vec<AvailableCommand>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 一条可用命令。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvailableCommand {
    /// 命令名。
    pub name: String,
    /// 描述。
    pub description: String,
    /// 可选输入说明（结构较深，按原始对象保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 当前模式更新。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentModeUpdate {
    /// 模式标识。
    #[serde(rename = "currentModeId")]
    pub current_mode_id: String,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 配置选项更新。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigOptionUpdate {
    /// 配置选项（select/boolean 的联合，按原始对象保留以避免过度建模）。
    #[serde(rename = "configOptions")]
    pub config_options: Vec<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 会话元信息更新。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionInfoUpdate {
    /// 标题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 更新时间。
    #[serde(rename = "updatedAt", default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// 上下文窗口与成本更新。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageUpdate {
    /// 已用 token。
    pub used: i64,
    /// 窗口大小。
    pub size: i64,
    /// 可选成本（`amount`/`currency`，按原始对象保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Value>,
    /// `_meta`。
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}
