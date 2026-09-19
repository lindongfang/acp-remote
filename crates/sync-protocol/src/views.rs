//! `event.payload.view` 的类型化视图（`schemas/sync/v1/event-views.schema.json`）。
//!
//! 每个 `$defs` 条目对应一个公开 struct，名字是 eventType 的驼峰化；[`VIEW_TYPES`] 登记全部 34 个
//! eventType，[`project`] 按 eventType 把 `payload.view` 投影成 [`View`]。
//!
//! 视图是**开放对象**（schema 的 `additionalProperties: true`）：因此
//!
//! - 每个视图 struct 都**不**加 `deny_unknown_fields`，并带 `#[serde(flatten)] pub extra: ExtraFields`，
//!   未知键按值保留（键序与空白由 serde 的 flatten 缓冲规范化，见 `common::ExtraFields` 的说明）；
//! - 内嵌的开放对象（`session.plan.changed.entries[]`、`session.commands.changed.commands[]`）同理。
//!
//! schema 里 `required` 的字段全部出现，`T | null` 用 [`Nullable`]（键必须存在）；`{"type": "object"}`
//! 用 [`RawObject`]（字节保真），`{"type": ["object", "null"]}` 用 `Nullable<Map<_, _>>`——不能用
//! `Nullable<RawObject>`，因为 `Nullable` 的非 null 路径会经 `serde_json::Value` 缓冲，而 `RawObject`
//! 持有的 `RawValue` 无法从 `Value` 反序列化。

use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::common::{
    AgentContentBlock, ConfigOptionView, DecimalString, ExtraFields, InteractionOption,
    NonEmptyText, Nullable, PublicError, RawObject, SessionSummary, Text, Timestamp, Uuid,
    ValueError, deserialize_optional_non_null,
};

/// 视图封闭 enum 与 Rust 镜像的登记表：`(eventType, JSON Pointer, 取值序列)`。
///
/// 指针从 `event-views.schema.json` 的 `$defs[eventType]` 出发（RFC 6901，以 `/` 开头）——`session.
/// plan.changed` 的枚举嵌在 `entries.items` 里，因此这里是指针而不是单个属性名。取值序列必须与
/// schema 的 `enum` **逐条相等且同序**。
///
/// `tests/schema_drift.rs::view_enums_match_schema` 双向校验这张表：已登记条目必须等于 schema，
/// 且 schema 里出现的每个 `enum` 都必须被登记。因此新增/修改视图封闭 enum 时，**要么**同步这里的
/// 取值并保证 Rust 镜像的 `ALL`/`as_str` 同序，**要么**门禁失败——不存在"Rust 静默落后于视图
/// schema"的中间状态。
pub const VIEW_ENUMS: &[(&str, &str, &[&str])] = &[
    (
        "session.plan.changed",
        "/properties/entries/items/properties/priority",
        &["high", "medium", "low"],
    ),
    (
        "session.plan.changed",
        "/properties/entries/items/properties/status",
        &["pending", "in_progress", "completed"],
    ),
    (
        "elicitation.resolved",
        "/properties/action",
        &["submit", "decline", "cancel"],
    ),
    (
        "terminal.output",
        "/properties/stream",
        &["stdout", "stderr"],
    ),
];

/// `session.plan.changed.entries[].priority`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlanPriority {
    High,
    Medium,
    Low,
}

impl PlanPriority {
    pub const ALL: [PlanPriority; 3] =
        [PlanPriority::High, PlanPriority::Medium, PlanPriority::Low];

    pub fn as_str(self) -> &'static str {
        match self {
            PlanPriority::High => "high",
            PlanPriority::Medium => "medium",
            PlanPriority::Low => "low",
        }
    }
}

impl fmt::Display for PlanPriority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PlanPriority {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        PlanPriority::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "priority",
                value: text.to_owned(),
            })
    }
}

impl Serialize for PlanPriority {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PlanPriority {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<Self>().map_err(DeError::custom)
    }
}

/// `session.plan.changed.entries[].status`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
}

impl PlanStatus {
    pub const ALL: [PlanStatus; 3] = [
        PlanStatus::Pending,
        PlanStatus::InProgress,
        PlanStatus::Completed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PlanStatus::Pending => "pending",
            PlanStatus::InProgress => "in_progress",
            PlanStatus::Completed => "completed",
        }
    }
}

impl fmt::Display for PlanStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PlanStatus {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        PlanStatus::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "status",
                value: text.to_owned(),
            })
    }
}

impl Serialize for PlanStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PlanStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<Self>().map_err(DeError::custom)
    }
}

/// `terminal.output.stream`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalStream {
    Stdout,
    Stderr,
}

impl TerminalStream {
    pub const ALL: [TerminalStream; 2] = [TerminalStream::Stdout, TerminalStream::Stderr];

    pub fn as_str(self) -> &'static str {
        match self {
            TerminalStream::Stdout => "stdout",
            TerminalStream::Stderr => "stderr",
        }
    }
}

impl fmt::Display for TerminalStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for TerminalStream {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        TerminalStream::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "stream",
                value: text.to_owned(),
            })
    }
}

impl Serialize for TerminalStream {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TerminalStream {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<Self>().map_err(DeError::custom)
    }
}

/// `elicitation.resolved.action`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElicitationAction {
    Submit,
    Decline,
    Cancel,
}

impl ElicitationAction {
    pub const ALL: [ElicitationAction; 3] = [
        ElicitationAction::Submit,
        ElicitationAction::Decline,
        ElicitationAction::Cancel,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ElicitationAction::Submit => "submit",
            ElicitationAction::Decline => "decline",
            ElicitationAction::Cancel => "cancel",
        }
    }
}

impl fmt::Display for ElicitationAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ElicitationAction {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        ElicitationAction::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "action",
                value: text.to_owned(),
            })
    }
}

impl Serialize for ElicitationAction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ElicitationAction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<Self>().map_err(DeError::custom)
    }
}

/// `turn.failed.state`：schema 的 `{"const": "failed"}`；失败态不接受任何别的字面量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnFailedState;

impl TurnFailedState {
    /// 该字段唯一合法的字面量。
    pub const WIRE: &'static str = "failed";
}

impl Serialize for TurnFailedState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(TurnFailedState::WIRE)
    }
}

impl<'de> Deserialize<'de> for TurnFailedState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == TurnFailedState::WIRE {
            Ok(Self)
        } else {
            Err(DeError::custom(ValueError::Enumerated {
                field: "state",
                value: text,
            }))
        }
    }
}

/// `command.uncertain.mayHaveReachedAgent`：schema 的 `{"const": true}`。
///
/// 它存在的意义就是让接收方无法把"可能已到达"读成"未到达"，因此 `false` 必须被拒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MayHaveReachedAgent;

impl Serialize for MayHaveReachedAgent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(true)
    }
}

impl<'de> Deserialize<'de> for MayHaveReachedAgent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if bool::deserialize(deserializer)? {
            Ok(Self)
        } else {
            Err(DeError::custom(ValueError::Enumerated {
                field: "mayHaveReachedAgent",
                value: "false".to_owned(),
            }))
        }
    }
}

/// `session.plan.changed.entries[]`（开放对象）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEntry {
    pub content: Text<4096>,
    pub priority: PlanPriority,
    pub status: PlanStatus,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `session.commands.changed.commands[]`（开放对象）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCommand {
    pub name: NonEmptyText<256>,
    pub description: Text<1024>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `agent.connected`（`event-views.schema.json#/$defs/agent.connected`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConnected {
    #[serde(rename = "agentId")]
    pub agent_id: NonEmptyText<128>,
    pub state: NonEmptyText<32>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `agent.disconnected`（`event-views.schema.json#/$defs/agent.disconnected`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDisconnected {
    #[serde(rename = "agentId")]
    pub agent_id: NonEmptyText<128>,
    pub state: NonEmptyText<32>,
    /// 非 required：键缺失即无错误详情；键存在时 schema 只允许 `publicError`，不接受 `null`。
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub error: Option<PublicError>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `agent.message.completed`（`event-views.schema.json#/$defs/agent.message.completed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessageCompleted {
    #[serde(rename = "messageId")]
    pub message_id: Uuid,
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub content: Vec<AgentContentBlock>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `agent.message.delta`（`event-views.schema.json#/$defs/agent.message.delta`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessageDelta {
    #[serde(rename = "messageId")]
    pub message_id: Uuid,
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    #[serde(rename = "deltaIndex")]
    pub delta_index: DecimalString,
    pub text: Text<262144>,
    /// 非 required：键缺失即该 chunk 只有文本投影（`text` 始终是投影，纯文本客户端只读它）；
    /// 键存在时 schema 只允许 `agentContentBlock`，不接受 `null`。
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub block: Option<AgentContentBlock>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `agent.thought.delta`（`event-views.schema.json#/$defs/agent.thought.delta`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentThoughtDelta {
    #[serde(rename = "messageId")]
    pub message_id: Uuid,
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    #[serde(rename = "deltaIndex")]
    pub delta_index: DecimalString,
    pub text: Text<262144>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `command.completed`（`event-views.schema.json#/$defs/command.completed`）。
///
/// `result` 是 `{"type": ["object", "null"]}`：用 `Nullable<Map<_, _>>` 而不是 `Nullable<RawObject>`，
/// 因为 `Nullable` 的非 null 路径经 `serde_json::Value` 缓冲，`RawObject` 无法从 `Value` 反序列化。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandCompleted {
    #[serde(rename = "requestId")]
    pub request_id: Uuid,
    pub result: Nullable<Map<String, Value>>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `command.failed`（`event-views.schema.json#/$defs/command.failed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandFailed {
    #[serde(rename = "requestId")]
    pub request_id: Uuid,
    pub error: PublicError,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `command.uncertain`（`event-views.schema.json#/$defs/command.uncertain`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandUncertain {
    #[serde(rename = "requestId")]
    pub request_id: Uuid,
    pub reason: NonEmptyText<256>,
    #[serde(rename = "mayHaveReachedAgent")]
    pub may_have_reached_agent: MayHaveReachedAgent,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `device.revoked`（`event-views.schema.json#/$defs/device.revoked`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRevoked {
    #[serde(rename = "deviceId")]
    pub device_id: Uuid,
    #[serde(rename = "revokedAt")]
    pub revoked_at: Timestamp,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `elicitation.requested`（`event-views.schema.json#/$defs/elicitation.requested`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationRequested {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub title: Text<1024>,
    /// 表单 schema：schema 里是 `{"type": "object"}`，字节保真。
    pub schema: RawObject,
    /// `{"type": ["object", "null"]}`，与 `command.completed.result` 同理不用 `RawObject`。
    #[serde(rename = "initialValues")]
    pub initial_values: Nullable<Map<String, Value>>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `elicitation.resolved`（`event-views.schema.json#/$defs/elicitation.resolved`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationResolved {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    pub action: ElicitationAction,
    #[serde(rename = "resolvedByDeviceId")]
    pub resolved_by_device_id: Nullable<Uuid>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `file.changed`（`event-views.schema.json#/$defs/file.changed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChanged {
    #[serde(rename = "changeId")]
    pub change_id: Uuid,
    pub kind: NonEmptyText<64>,
    #[serde(rename = "displayPath")]
    pub display_path: NonEmptyText<1024>,
    pub summary: Text<2048>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `permission.requested`（`event-views.schema.json#/$defs/permission.requested`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequested {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub title: Text<1024>,
    pub description: Nullable<Text<4096>>,
    pub options: Vec<InteractionOption>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `permission.resolved`（`event-views.schema.json#/$defs/permission.resolved`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionResolved {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    pub resolution: NonEmptyText<128>,
    #[serde(rename = "resolvedByDeviceId")]
    pub resolved_by_device_id: Nullable<Uuid>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `session.commands.changed`（`event-views.schema.json#/$defs/session.commands.changed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCommandsChanged {
    pub commands: Vec<SessionCommand>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `session.config.changed`（`event-views.schema.json#/$defs/session.config.changed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfigChanged {
    #[serde(rename = "configOptions")]
    pub config_options: Vec<ConfigOptionView>,
    pub version: DecimalString,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `session.created`（`event-views.schema.json#/$defs/session.created`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreated {
    pub session: SessionSummary,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `session.info.changed`（`event-views.schema.json#/$defs/session.info.changed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfoChanged {
    pub title: Nullable<Text<512>>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Timestamp,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `session.mode.changed`（`event-views.schema.json#/$defs/session.mode.changed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionModeChanged {
    #[serde(rename = "currentModeId")]
    pub current_mode_id: Nullable<NonEmptyText<256>>,
    pub version: DecimalString,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `session.origin.online_changed`（`event-views.schema.json#/$defs/session.origin.online_changed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOriginOnlineChanged {
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "exportId")]
    pub export_id: NonEmptyText<256>,
    #[serde(rename = "originEpoch")]
    pub origin_epoch: Uuid,
    pub online: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `session.plan.changed`（`event-views.schema.json#/$defs/session.plan.changed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPlanChanged {
    pub entries: Vec<PlanEntry>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `session.updated`（`event-views.schema.json#/$defs/session.updated`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUpdated {
    pub session: SessionSummary,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `session.usage.changed`（`event-views.schema.json#/$defs/session.usage.changed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUsageChanged {
    pub used: DecimalString,
    pub size: DecimalString,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `terminal.output`（`event-views.schema.json#/$defs/terminal.output`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalOutput {
    #[serde(rename = "terminalId")]
    pub terminal_id: NonEmptyText<256>,
    #[serde(rename = "chunkIndex")]
    pub chunk_index: DecimalString,
    pub stream: TerminalStream,
    pub text: Text<1048576>,
    pub truncated: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `tool.call.completed`（`event-views.schema.json#/$defs/tool.call.completed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallCompleted {
    #[serde(rename = "toolCallId")]
    pub tool_call_id: NonEmptyText<256>,
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub title: Text<1024>,
    pub state: NonEmptyText<32>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `tool.call.started`（`event-views.schema.json#/$defs/tool.call.started`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallStarted {
    #[serde(rename = "toolCallId")]
    pub tool_call_id: NonEmptyText<256>,
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub title: Text<1024>,
    pub state: NonEmptyText<32>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `tool.call.updated`（`event-views.schema.json#/$defs/tool.call.updated`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallUpdated {
    #[serde(rename = "toolCallId")]
    pub tool_call_id: NonEmptyText<256>,
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub title: Text<1024>,
    pub state: NonEmptyText<32>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `turn.cancelled`（`event-views.schema.json#/$defs/turn.cancelled`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCancelled {
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub state: NonEmptyText<32>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `turn.completed`（`event-views.schema.json#/$defs/turn.completed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCompleted {
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub state: NonEmptyText<32>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `turn.delta_compacted`（`event-views.schema.json#/$defs/turn.delta_compacted`）。
///
/// summary 事件**只承载收据**（`turnId` + `deltaCount`），不复制正文：它替代被压缩掉的那些
/// `kind='delta'` 事件出现在重放里，终态正文一律以 `agent.message.completed` 为准
/// （`docs/SYNC_PROTOCOL.md` §10.3(c)、`docs/CORE_PORTS_AND_STORAGE.md` §6 第 15 条）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnDeltaCompacted {
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    #[serde(rename = "deltaCount")]
    pub delta_count: DecimalString,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `turn.failed`（`event-views.schema.json#/$defs/turn.failed`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnFailed {
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub state: TurnFailedState,
    pub error: PublicError,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `turn.queued`（`event-views.schema.json#/$defs/turn.queued`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnQueued {
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub state: NonEmptyText<32>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `turn.started`（`event-views.schema.json#/$defs/turn.started`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnStarted {
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    pub state: NonEmptyText<32>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `user.message.delta`（`event-views.schema.json#/$defs/user.message.delta`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessageDelta {
    #[serde(rename = "messageId")]
    pub message_id: Uuid,
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    #[serde(rename = "deltaIndex")]
    pub delta_index: DecimalString,
    pub text: Text<262144>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `eventType` → 视图类型的绑定表，同时生成 [`VIEW_TYPES`]、[`View`] 与 [`project`]。
///
/// 三者由同一条表生成，因此不会出现"登记了 eventType 但 `project` 不认"的漂移。顺序是
/// `serde_json::Map`（本 workspace 未启用 `preserve_order`，即 `BTreeMap`）迭代
/// `event-views.schema.json` 的 `$defs` 得到的键序，也就是
/// `tests/schema_drift.rs::view_types_match_schema_defs` 逐条比较的那个顺序。
macro_rules! view_registry {
    ($( $event_type:literal => $variant:ident($ty:ty) ),+ $(,)?) => {
        /// 全部 34 个登记 eventType，与 `event-views.schema.json` 的 `$defs` 键集合逐条相等。
        pub const VIEW_TYPES: [&str; 34] = [ $( $event_type ),+ ];

        /// 投影结果：一个登记 eventType 一个变体，负载是对应的类型化视图。
        #[derive(Debug, Clone, Serialize)]
        #[serde(untagged)]
        pub enum View {
            $( $variant($ty) ),+
        }

        impl View {
            /// 本视图对应的 `eventType`（[`VIEW_TYPES`] 的条目）。
            pub fn event_type(&self) -> &'static str {
                match self {
                    $( View::$variant(_) => $event_type ),+
                }
            }
        }

        /// 按 `eventType` 把 `payload.view` 投影成类型化视图。
        ///
        /// 未登记的 `eventType` 一律拒绝（`ValueError::Enumerated`）：v1 没有"未知事件"的通用
        /// 视图，未知类型由调用方按 `docs/SYNC_PROTOCOL.md` §10.1 降级处理，不得在这里假装理解它。
        pub fn project(event_type: &str, view: &RawObject) -> Result<View, ValueError> {
            match event_type {
                $(
                    $event_type => project_value::<$ty>(view).map(View::$variant),
                )+
                other => Err(ValueError::Enumerated {
                    field: "eventType",
                    value: other.to_owned(),
                }),
            }
        }
    };
}

view_registry! {
    "agent.connected" => AgentConnected(AgentConnected),
    "agent.disconnected" => AgentDisconnected(AgentDisconnected),
    "agent.message.completed" => AgentMessageCompleted(AgentMessageCompleted),
    "agent.message.delta" => AgentMessageDelta(AgentMessageDelta),
    "agent.thought.delta" => AgentThoughtDelta(AgentThoughtDelta),
    "command.completed" => CommandCompleted(CommandCompleted),
    "command.failed" => CommandFailed(CommandFailed),
    "command.uncertain" => CommandUncertain(CommandUncertain),
    "device.revoked" => DeviceRevoked(DeviceRevoked),
    "elicitation.requested" => ElicitationRequested(ElicitationRequested),
    "elicitation.resolved" => ElicitationResolved(ElicitationResolved),
    "file.changed" => FileChanged(FileChanged),
    "permission.requested" => PermissionRequested(PermissionRequested),
    "permission.resolved" => PermissionResolved(PermissionResolved),
    "session.commands.changed" => SessionCommandsChanged(SessionCommandsChanged),
    "session.config.changed" => SessionConfigChanged(SessionConfigChanged),
    "session.created" => SessionCreated(SessionCreated),
    "session.info.changed" => SessionInfoChanged(SessionInfoChanged),
    "session.mode.changed" => SessionModeChanged(SessionModeChanged),
    "session.origin.online_changed" => SessionOriginOnlineChanged(SessionOriginOnlineChanged),
    "session.plan.changed" => SessionPlanChanged(SessionPlanChanged),
    "session.updated" => SessionUpdated(SessionUpdated),
    "session.usage.changed" => SessionUsageChanged(SessionUsageChanged),
    "terminal.output" => TerminalOutput(TerminalOutput),
    "tool.call.completed" => ToolCallCompleted(ToolCallCompleted),
    "tool.call.started" => ToolCallStarted(ToolCallStarted),
    "tool.call.updated" => ToolCallUpdated(ToolCallUpdated),
    "turn.cancelled" => TurnCancelled(TurnCancelled),
    "turn.completed" => TurnCompleted(TurnCompleted),
    "turn.delta_compacted" => TurnDeltaCompacted(TurnDeltaCompacted),
    "turn.failed" => TurnFailed(TurnFailed),
    "turn.queued" => TurnQueued(TurnQueued),
    "turn.started" => TurnStarted(TurnStarted),
    "user.message.delta" => UserMessageDelta(UserMessageDelta),
}

/// 把 `payload.view` 的原文投影成 `T`。
fn project_value<T: serde::de::DeserializeOwned>(view: &RawObject) -> Result<T, ValueError> {
    serde_json::from_str::<T>(view.get()).map_err(view_shape_error)
}

/// 把 `serde_json` 的形状诊断映射成合同错误。
///
/// `detail` 保留 serde 的原文（缺哪个字段、哪个字段类型不符），调用方不必再去猜；
/// 这是让"投影失败"可被判据化的唯一入口。
fn view_shape_error(error: serde_json::Error) -> ValueError {
    ValueError::ViewShape {
        field: "view",
        detail: error.to_string(),
    }
}
