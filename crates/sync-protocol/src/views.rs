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
    ("agent.connected", "/properties/state", &["connected"]),
    ("agent.disconnected", "/properties/state", &["disconnected"]),
    (
        "file.changed",
        "/properties/diffLines/items/properties/kind",
        &["added", "deleted", "context"],
    ),
    (
        "file.changed",
        "/properties/diffUnavailable/properties/reason",
        &["size_limit"],
    ),
    (
        "tool.call.started",
        "/properties/toolDetail/properties/truncatedParts/items",
        &["input", "output", "text"],
    ),
    (
        "tool.call.updated",
        "/properties/toolDetail/properties/truncatedParts/items",
        &["input", "output", "text"],
    ),
    (
        "tool.call.completed",
        "/properties/toolDetail/properties/truncatedParts/items",
        &["input", "output", "text"],
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

/// `agent.connected.state`：封闭词表只有一个取值，客户端可据此把连接事件与断开事件区分开。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentConnectedState {
    Connected,
}

impl AgentConnectedState {
    pub const ALL: [AgentConnectedState; 1] = [AgentConnectedState::Connected];

    pub fn as_str(self) -> &'static str {
        match self {
            AgentConnectedState::Connected => "connected",
        }
    }
}

impl fmt::Display for AgentConnectedState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AgentConnectedState {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        AgentConnectedState::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "state",
                value: text.to_owned(),
            })
    }
}

impl Serialize for AgentConnectedState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AgentConnectedState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<Self>().map_err(DeError::custom)
    }
}

/// `agent.disconnected.state`：与 [`AgentConnectedState`] 互不相同的唯一取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentDisconnectedState {
    Disconnected,
}

impl AgentDisconnectedState {
    pub const ALL: [AgentDisconnectedState; 1] = [AgentDisconnectedState::Disconnected];

    pub fn as_str(self) -> &'static str {
        match self {
            AgentDisconnectedState::Disconnected => "disconnected",
        }
    }
}

impl fmt::Display for AgentDisconnectedState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AgentDisconnectedState {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        AgentDisconnectedState::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "state",
                value: text.to_owned(),
            })
    }
}

impl Serialize for AgentDisconnectedState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AgentDisconnectedState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<Self>().map_err(DeError::custom)
    }
}

/// `file.changed.diffLines[].kind`：结构化差异行的判别子（feature `core.file-diff.v1` 门控）。
///
/// 客户端据此给每一行加前缀与配色，**不必**解析任何原文；取值集合是封闭词表，未登记的取值不是
/// 「另一种行」而是形状错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileDiffLineKind {
    Added,
    Deleted,
    Context,
}

impl FileDiffLineKind {
    pub const ALL: [FileDiffLineKind; 3] = [
        FileDiffLineKind::Added,
        FileDiffLineKind::Deleted,
        FileDiffLineKind::Context,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FileDiffLineKind::Added => "added",
            FileDiffLineKind::Deleted => "deleted",
            FileDiffLineKind::Context => "context",
        }
    }
}

impl fmt::Display for FileDiffLineKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for FileDiffLineKind {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        FileDiffLineKind::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "kind",
                value: text.to_owned(),
            })
    }
}

impl Serialize for FileDiffLineKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FileDiffLineKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<Self>().map_err(DeError::custom)
    }
}

/// `file.changed.diffUnavailable.reason`：差异行未随事件下发的**已登记**原因（封闭词表）。
///
/// v1 只登记一个原因——单处改动的差异行数或字节数超过固定上限（`docs/SYNC_PROTOCOL.md` §14）；
/// 客户端据此呈现明确说明，MUST NOT 把省略读成「该处没有内容」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileDiffUnavailableReason {
    SizeLimit,
}

impl FileDiffUnavailableReason {
    pub const ALL: [FileDiffUnavailableReason; 1] = [FileDiffUnavailableReason::SizeLimit];

    pub fn as_str(self) -> &'static str {
        match self {
            FileDiffUnavailableReason::SizeLimit => "size_limit",
        }
    }
}

impl fmt::Display for FileDiffUnavailableReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for FileDiffUnavailableReason {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        FileDiffUnavailableReason::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "reason",
                value: text.to_owned(),
            })
    }
}

impl Serialize for FileDiffUnavailableReason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FileDiffUnavailableReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<Self>().map_err(DeError::custom)
    }
}

/// `file.changed.diffLines` 的一行（`event-views.schema.json#/$defs/file.changed.properties.diffLines.
/// items`）：判别子加**不带前缀**的行文本。行文本是结构化数据而不是拼好的文本块，客户端按 `kind`
/// 自行加 `+ `/`- `/`  ` 前缀。
///
/// 与其它视图内嵌对象一致：`additionalProperties: true`，未知键经 [`ExtraFields`] 原样保留。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiffLine {
    pub kind: FileDiffLineKind,
    pub text: Text<4096>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `file.changed.diffUnavailable`：差异行未下发的显式标记（feature `core.file-diff.v1` 门控）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiffUnavailable {
    pub reason: FileDiffUnavailableReason,
    #[serde(flatten)]
    pub extra: ExtraFields,
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
    pub state: AgentConnectedState,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// `agent.disconnected`（`event-views.schema.json#/$defs/agent.disconnected`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDisconnected {
    #[serde(rename = "agentId")]
    pub agent_id: NonEmptyText<128>,
    pub state: AgentDisconnectedState,
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
///
/// `addedLines`/`deletedLines`/`outsideWorkspace` 是可选字段，schema 的 `required` 集合不含它们。
///
/// 字段级属性只服务 `Serialize`；反序列化由 [`RawFileChanged`] 的表单承担，因为 `diffLines` 与
/// `diffUnavailable` 的**互斥**（schema 的 `not`）是跨字段约束——手法与 [`crate::event::Body`] 的
/// `try_from` 相同。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RawFileChanged")]
pub struct FileChanged {
    #[serde(rename = "changeId")]
    pub change_id: Uuid,
    pub kind: NonEmptyText<64>,
    #[serde(rename = "displayPath")]
    pub display_path: NonEmptyText<1024>,
    pub summary: Text<2048>,
    /// 非 required：键缺失即「本次工具调用判定不出行数」，不得读成零改动；
    /// 键存在时 schema 只允许 `decimalString`，不接受 `null`。
    #[serde(rename = "addedLines", skip_serializing_if = "Option::is_none")]
    pub added_lines: Option<DecimalString>,
    /// 非 required：键缺失即「本次工具调用判定不出行数」，不得读成零改动；
    /// 键存在时 schema 只允许 `decimalString`，不接受 `null`。
    #[serde(rename = "deletedLines", skip_serializing_if = "Option::is_none")]
    pub deleted_lines: Option<DecimalString>,
    /// 非 required：键缺失即路径在该会话工作区之内；
    /// 键存在时 schema 只允许 `boolean`，不接受 `null`。
    #[serde(rename = "outsideWorkspace", skip_serializing_if = "Option::is_none")]
    pub outside_workspace: Option<bool>,
    /// 结构化差异行（feature `core.file-diff.v1` 门控）：未协商该 feature 时键**缺席**（不是空数组），
    /// 差异行因超限未下发时同样缺席并改由 [`Self::diff_unavailable`] 说明原因。
    #[serde(rename = "diffLines", skip_serializing_if = "Option::is_none")]
    pub diff_lines: Option<Vec<FileDiffLine>>,
    /// 差异行未随本事件下发时的显式标记；与 [`Self::diff_lines`] 互斥。
    #[serde(rename = "diffUnavailable", skip_serializing_if = "Option::is_none")]
    pub diff_unavailable: Option<FileDiffUnavailable>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// [`FileChanged`] 的线形状：字段与 [`FileChanged`] 一一对应，「键可缺省」的 `default` 与
/// 「键在场时不得为 `null`」的 `deserialize_with` 都落在这一侧。视图是**开放对象**，因此这里
/// **不**加 `deny_unknown_fields`——未知键照旧经 `extra` 原样保留。
#[derive(Debug, Deserialize)]
struct RawFileChanged {
    #[serde(rename = "changeId")]
    change_id: Uuid,
    kind: NonEmptyText<64>,
    #[serde(rename = "displayPath")]
    display_path: NonEmptyText<1024>,
    summary: Text<2048>,
    #[serde(
        rename = "addedLines",
        default,
        deserialize_with = "deserialize_optional_non_null"
    )]
    added_lines: Option<DecimalString>,
    #[serde(
        rename = "deletedLines",
        default,
        deserialize_with = "deserialize_optional_non_null"
    )]
    deleted_lines: Option<DecimalString>,
    #[serde(
        rename = "outsideWorkspace",
        default,
        deserialize_with = "deserialize_optional_non_null"
    )]
    outside_workspace: Option<bool>,
    #[serde(
        rename = "diffLines",
        default,
        deserialize_with = "deserialize_optional_non_null"
    )]
    diff_lines: Option<Vec<FileDiffLine>>,
    #[serde(
        rename = "diffUnavailable",
        default,
        deserialize_with = "deserialize_optional_non_null"
    )]
    diff_unavailable: Option<FileDiffUnavailable>,
    #[serde(flatten)]
    extra: ExtraFields,
}

/// `diffLines` 与 `diffUnavailable` 互斥（`event-views.schema.json#/$defs/file.changed` 的 `not`，
/// `docs/SYNC_PROTOCOL.md` §10.3）：同时携带两者会让消费方同时收到「逐行差异」与「差异未下发」两条
/// 相反信号，因此按形状错误拒绝，而不是让其中一个静默胜出。
impl TryFrom<RawFileChanged> for FileChanged {
    type Error = ValueError;

    fn try_from(raw: RawFileChanged) -> Result<Self, Self::Error> {
        if raw.diff_lines.is_some() && raw.diff_unavailable.is_some() {
            return Err(ValueError::Shape {
                field: "view",
                expected: "diffLines 与 diffUnavailable 不得同时出现（event-views.schema.json#/$defs/file.changed 的 not）",
            });
        }
        Ok(Self {
            change_id: raw.change_id,
            kind: raw.kind,
            display_path: raw.display_path,
            summary: raw.summary,
            added_lines: raw.added_lines,
            deleted_lines: raw.deleted_lines,
            outside_workspace: raw.outside_workspace,
            diff_lines: raw.diff_lines,
            diff_unavailable: raw.diff_unavailable,
            extra: raw.extra,
        })
    }
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

/// `toolDetail.truncatedParts[]`：因超过 `docs/SYNC_PROTOCOL.md` §14 的固定上限而**整体缺席**的部分名。
///
/// 这是封闭词表：客户端据此对相应部分给出明确的截断说明，MUST NOT 把缺席读成「没有内容」，也
/// MUST NOT 把未登记的取值当成另一种截断。取值序列必须与 `event-views.schema.json` 三个
/// `tool.call.*` 的 `properties.toolDetail.properties.truncatedParts.items.enum` 逐条相等。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolDetailPart {
    Input,
    Output,
    Text,
}

impl ToolDetailPart {
    pub const ALL: [ToolDetailPart; 3] = [
        ToolDetailPart::Input,
        ToolDetailPart::Output,
        ToolDetailPart::Text,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ToolDetailPart::Input => "input",
            ToolDetailPart::Output => "output",
            ToolDetailPart::Text => "text",
        }
    }
}

impl fmt::Display for ToolDetailPart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ToolDetailPart {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        ToolDetailPart::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "truncatedParts",
                value: text.to_owned(),
            })
    }
}

impl Serialize for ToolDetailPart {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ToolDetailPart {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse::<Self>().map_err(DeError::custom)
    }
}

/// `toolDetail`（`event-views.schema.json#/$defs/tool.call.started.properties.toolDetail`，三个
/// `tool.call.*` 定义同形）：
/// 工具调用的输入、输出与文本内容，供客户端在折叠块里展示；ACP 原文仍逐字节保留在 `acp`。
///
/// 四个键都**可缺席**，且缺席是唯一合法的「无此部分」表达（下发的 MUST NOT 是 `null`、空对象或空
/// 数组）。`input`/`output` 是内嵌的 [`Value`]：它们是 ACP `rawInput`/`rawOutput` 的 JSON 值，
/// 视图层不进一步约束形状，只保证未被改写。
///
/// [`Self::input`]/[`Self::output`]/[`Self::text`] 的上限与截断由生产方（`agent-host` 映射层）执行：
/// 某一部分超过 `docs/SYNC_PROTOCOL.md` §14 的 32 KiB 时**整体缺席**，并把部分名登记在
/// [`Self::truncated_parts`]；绝不发送被截断的 JSON 片段。schema 只能描述形状，字节预算无法在
/// schema 里表达。
///
/// 无法以 ACPR-CJ1 值域表达的部分（例如 `input`/`output` 含浮点或超出 2^53-1 的整数）同样
/// **整体缺席**并登记在 [`Self::truncated_parts`]：`view` 落库前要过 ACPR-CJ1，宁可少带一个值，
/// 也不让整条事件因 view 校验失败而无法提交。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDetail {
    /// ACP `rawInput` 的 JSON 值；Agent 未提供时缺席。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    /// ACP `rawOutput` 的 JSON 值；Agent 未提供时缺席。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    /// ACP 工具内容里文本块（`ToolCallContent::Content` → `ContentBlock::Text`）的文本，按 Agent
    /// 提供的顺序排列；没有文本块时缺席（不是空数组）。diff、terminal 与未识别的 content 不进入
    /// 这里。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Vec<String>>,
    /// 因超限而整体缺席的部分名；没有截断时缺席（不是空数组）。
    #[serde(
        rename = "truncatedParts",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub truncated_parts: Option<Vec<ToolDetailPart>>,
    /// `toolDetail` 同样是开放对象（schema 的 `additionalProperties: true`）：未知扩展键原样保留，
    /// 与三个 `tool.call.*` 视图的 [`ExtraFields`] 同一口径。
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
    /// 非 required：Agent 没有提供输入、输出或文本内容时**整个键缺席**，视图仍完整。
    #[serde(
        rename = "toolDetail",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_detail: Option<ToolCallDetail>,
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
    /// 非 required：Agent 没有提供输入、输出或文本内容时**整个键缺席**，视图仍完整。
    #[serde(
        rename = "toolDetail",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_detail: Option<ToolCallDetail>,
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
    /// 非 required：Agent 没有提供输入、输出或文本内容时**整个键缺席**，视图仍完整。
    #[serde(
        rename = "toolDetail",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_detail: Option<ToolCallDetail>,
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
