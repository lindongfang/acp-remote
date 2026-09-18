//! `command` 与 `command.result` 的 body（`schemas/sync/v1/command.schema.json`、
//! `docs/SYNC_PROTOCOL.md` §11）。
//!
//! 信封（`protocolVersion`/`type`/`messageId`/`connectionId`/`connectionSequence`）由
//! [`crate::envelope`] 承载；本模块只承载 `body`，与 `auth`/`control`/`error` 三个家族一致。
//!
//! 命令名的唯一机器来源是 `compatibility/commands/v1/commands.json`：[`CommandName`] 是它
//! `transport` 含 `sync` 的 11 条命令的镜像（`session.create` 只经 Node Link 接受，Sync v1 收到时
//! 按 `command.unsupported` 拒绝）。
//!
//! 校验只发生在反序列化，且只执行 schema 能判定的结构：
//!
//! - `required` 与 `additionalProperties: false`，包括"哪些命令必须在 body 顶层携带 `sessionId`、
//!   哪些命令禁止它"这类**键是否出现**的规则（`docs/SYNC_PROTOCOL.md` §11.5 的表）；
//! - `command.result.body` 的 `allOf/if-then`：`command`（查询/变更）与 `status` 共同决定 `result`
//!   的形状，以及 `acceptedAt`/`terminalEventId`/`result`/`error` 的取值。
//!
//! 结构不符一律报 [`ValueError::Shape`]；`Deserialize` 返回 `Ok` 即表示该 body 满足 schema。

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use serde::de::DeserializeOwned;
use serde::de::Error as DeError;
use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::common::{
    ConfigOptionValue, ConfigOptionView, DecimalString, ModeState, NonEmptyText, Nullable,
    PromptContentBlock, PublicError, RawObject, SessionSummary, Timestamp, Uuid, ValueError,
    deserialize_optional_non_null,
};
use crate::sync::{
    SnapshotItemCapability, SnapshotItemMessage, SnapshotItemPendingInteraction, SnapshotItemTurn,
};

/// `requestId`（`command.schema.json#/$defs/requestId`）：该 def 是 `common.schema.json#/$defs/uuid`
/// 的 `$ref`，因此本类型就是 [`Uuid`]。
///
/// 命令的 `requestId` 同时是幂等键（`docs/SYNC_PROTOCOL.md` §11.1）：客户端在响应丢失后必须用同一个
/// id 重试，服务端必须在重复提交时返回相同的已知接受/终态。
pub type RequestId = Uuid;

/// `commandName`（`command.schema.json#/$defs/commandName`）。
///
/// 逐条等于 `compatibility/commands/v1/commands.json` 中 `transport` 含 `sync` 的 11 条命令，顺序
/// 与该 registry 一致（也是 `docs/SYNC_PROTOCOL.md` §11.5 首列去掉 `session.create` 的结果）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandName {
    /// `session.list`
    SessionList,
    /// `session.read`
    SessionRead,
    /// `command.status`
    CommandStatus,
    /// `session.mode.list`
    SessionModeList,
    /// `session.config.list`
    SessionConfigList,
    /// `session.prompt`
    SessionPrompt,
    /// `session.cancel`
    SessionCancel,
    /// `elicitation.respond`
    ElicitationRespond,
    /// `session.mode.set`
    SessionModeSet,
    /// `session.config.set`
    SessionConfigSet,
    /// `permission.resolve`
    PermissionResolve,
}

impl CommandName {
    /// 与 `compatibility/commands/v1/commands.json` 的 sync 子集逐条相等，顺序一致。
    pub const ALL: [CommandName; 11] = [
        CommandName::SessionList,
        CommandName::SessionRead,
        CommandName::CommandStatus,
        CommandName::SessionModeList,
        CommandName::SessionConfigList,
        CommandName::SessionPrompt,
        CommandName::SessionCancel,
        CommandName::ElicitationRespond,
        CommandName::SessionModeSet,
        CommandName::SessionConfigSet,
        CommandName::PermissionResolve,
    ];

    /// 查询命令（`docs/SYNC_PROTOCOL.md` §11.5 的"类别"列）：不进入异步队列，`command.result` 的
    /// `completed` 直接给出结果形状，且 `terminalEventId` 固定为 `null`。
    pub fn is_query(self) -> bool {
        matches!(
            self,
            CommandName::SessionList
                | CommandName::SessionRead
                | CommandName::CommandStatus
                | CommandName::SessionModeList
                | CommandName::SessionConfigList
        )
    }

    /// body 顶层是否**必须**携带 `sessionId`（各 def 的 `required` 列表；其余命令按
    /// `additionalProperties: false` 反过来禁止它）。
    pub fn requires_session_id(self) -> bool {
        !matches!(self, CommandName::SessionList | CommandName::CommandStatus)
    }

    /// body 顶层是否**必须**携带 `expectedVersion`（只有 `session.config.set` 与 `session.mode.set`）。
    pub fn requires_expected_version(self) -> bool {
        matches!(
            self,
            CommandName::SessionConfigSet | CommandName::SessionModeSet
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CommandName::SessionList => "session.list",
            CommandName::SessionRead => "session.read",
            CommandName::CommandStatus => "command.status",
            CommandName::SessionModeList => "session.mode.list",
            CommandName::SessionConfigList => "session.config.list",
            CommandName::SessionPrompt => "session.prompt",
            CommandName::SessionCancel => "session.cancel",
            CommandName::ElicitationRespond => "elicitation.respond",
            CommandName::SessionModeSet => "session.mode.set",
            CommandName::SessionConfigSet => "session.config.set",
            CommandName::PermissionResolve => "permission.resolve",
        }
    }
}

impl fmt::Display for CommandName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CommandName {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        CommandName::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "command",
                value: text.to_owned(),
            })
    }
}

impl Serialize for CommandName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CommandName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        CommandName::from_str(&text).map_err(DeError::custom)
    }
}

/// 命令生命周期状态：schema 里 `commandResult.body.status` 与 `commandStatusRecord.state` 两处
/// inline enum 的取值集合完全相同（列举顺序不同），因此共用一个 Rust 枚举。
///
/// `ALL` 采用 `commandStatusRecord.state` 的顺序（`docs/SYNC_PROTOCOL.md` §11.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandState {
    /// `accepted`：命令已持久化接受，异步进展通过事件发送。
    Accepted,
    /// `completed`：查询命令已完成，或重复查询一个已成功终结的 mutation。
    Completed,
    /// `failed`：已接受的 mutation 后续失败。
    Failed,
    /// `uncertain`：命令可能已到达外部 Agent，崩溃恢复后无法证明结果；是终态，不会自动重派。
    Uncertain,
    /// `rejected`：未产生业务副作用，`error` 携带结构化原因。
    Rejected,
}

impl CommandState {
    /// 两处 schema enum 的取值集合，顺序为 `commandStatusRecord.state`。
    pub const ALL: [CommandState; 5] = [
        CommandState::Accepted,
        CommandState::Completed,
        CommandState::Failed,
        CommandState::Uncertain,
        CommandState::Rejected,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CommandState::Accepted => "accepted",
            CommandState::Completed => "completed",
            CommandState::Failed => "failed",
            CommandState::Uncertain => "uncertain",
            CommandState::Rejected => "rejected",
        }
    }
}

impl fmt::Display for CommandState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CommandState {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        CommandState::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "status",
                value: text.to_owned(),
            })
    }
}

impl Serialize for CommandState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CommandState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        CommandState::from_str(&text).map_err(DeError::custom)
    }
}

/// `sessionList`（`command.schema.json#/$defs/sessionList`）的 `payload`：schema 要求空 object
/// （`docs/SYNC_PROTOCOL.md` §11.5 的 `{}`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionList {}

/// `session.read.payload.include` 的元素（schema 的 `items.enum`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IncludeResource {
    /// `messages`
    Messages,
    /// `turns`
    Turns,
    /// `pending_interactions`
    PendingInteractions,
    /// `config_options`
    ConfigOptions,
    /// `capabilities`
    Capabilities,
}

impl IncludeResource {
    /// schema 的 `items.enum`，顺序一致。
    pub const ALL: [IncludeResource; 5] = [
        IncludeResource::Messages,
        IncludeResource::Turns,
        IncludeResource::PendingInteractions,
        IncludeResource::ConfigOptions,
        IncludeResource::Capabilities,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            IncludeResource::Messages => "messages",
            IncludeResource::Turns => "turns",
            IncludeResource::PendingInteractions => "pending_interactions",
            IncludeResource::ConfigOptions => "config_options",
            IncludeResource::Capabilities => "capabilities",
        }
    }
}

impl fmt::Display for IncludeResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for IncludeResource {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        IncludeResource::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "include",
                value: text.to_owned(),
            })
    }
}

impl Serialize for IncludeResource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for IncludeResource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        IncludeResource::from_str(&text).map_err(DeError::custom)
    }
}

/// `session.read.payload.include`：去重的资源名列表（schema 的 `uniqueItems: true`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeList(Vec<IncludeResource>);

impl IncludeList {
    pub fn as_slice(&self) -> &[IncludeResource] {
        &self.0
    }

    fn validate(&self) -> Result<(), ValueError> {
        for (index, resource) in self.0.iter().enumerate() {
            if self.0[index + 1..]
                .iter()
                .any(|other| other.as_str() == resource.as_str())
            {
                return Err(ValueError::RepeatedItem {
                    field: "include",
                    value: resource.as_str().to_owned(),
                });
            }
        }
        Ok(())
    }
}

impl Serialize for IncludeList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for IncludeList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let list = Self(Vec::<IncludeResource>::deserialize(deserializer)?);
        list.validate().map_err(DeError::custom)?;
        Ok(list)
    }
}

/// `sessionRead`（`command.schema.json#/$defs/sessionRead`）的 `payload`：`{ include }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRead {
    pub include: IncludeList,
}

/// `session.prompt.payload.content`：`1..=64` 个 [`PromptContentBlock`]（schema 的 `minItems`/`maxItems`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptContentList(Vec<PromptContentBlock>);

impl PromptContentList {
    pub fn as_slice(&self) -> &[PromptContentBlock] {
        &self.0
    }

    fn validate(&self) -> Result<(), ValueError> {
        if self.0.is_empty() {
            return Err(ValueError::Shape {
                field: "content",
                expected: "至少 1 个 PromptContentBlock（schema 的 minItems: 1）",
            });
        }
        if self.0.len() > 64 {
            return Err(ValueError::TooManyItems {
                field: "content",
                max: 64,
                actual: self.0.len(),
            });
        }
        Ok(())
    }
}

impl Serialize for PromptContentList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PromptContentList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let list = Self(Vec::<PromptContentBlock>::deserialize(deserializer)?);
        list.validate().map_err(DeError::custom)?;
        Ok(list)
    }
}

/// `sessionPrompt`（`command.schema.json#/$defs/sessionPrompt`）的 `payload`：`{ content }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPrompt {
    pub content: PromptContentList,
}

/// `sessionCancel`（`command.schema.json#/$defs/sessionCancel`）的 `payload`：`{ turnId }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCancel {
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
}

/// `configList`（`command.schema.json#/$defs/configList`）的 `payload`：schema 要求空 object。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigList {}

/// `configSet`（`command.schema.json#/$defs/configSet`）的 `payload`：`{ configId, value }`。
///
/// `value` 的取值域是 `string | boolean`（与 `common.configOptionView.currentValue` 相同），
/// 因此复用 [`ConfigOptionValue`]；"该 option 的 `type` 决定用 string 还是 boolean" 是会话状态
/// 校验（`docs/SYNC_PROTOCOL.md` §11.5），不属于 wire 形状。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigSet {
    #[serde(rename = "configId")]
    pub config_id: NonEmptyText<256>,
    pub value: ConfigOptionValue,
}

/// `modeList`（`command.schema.json#/$defs/modeList`）的 `payload`：schema 要求空 object。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeList {}

/// `modeSet`（`command.schema.json#/$defs/modeSet`）的 `payload`：`{ modeId }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeSet {
    #[serde(rename = "modeId")]
    pub mode_id: NonEmptyText<256>,
}

/// `permissionResolve`（`command.schema.json#/$defs/permissionResolve`）的 `payload`：
/// `{ interactionId, optionId }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionResolve {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    #[serde(rename = "optionId")]
    pub option_id: NonEmptyText<128>,
}

/// `elicitation.respond.payload.action`（schema 的三个分支的 `const`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElicitationAction {
    /// `submit`：`values` 必须是 object 或 `null`（对齐 ACP `accept` 的 `content: null`）。
    Submit,
    /// `decline`：`values` 必须是 `null`。
    Decline,
    /// `cancel`：`values` 必须是 `null`。
    Cancel,
}

impl ElicitationAction {
    /// 三个分支的取值，顺序与 schema 一致。
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
        ElicitationAction::from_str(&text).map_err(DeError::custom)
    }
}

/// `elicitationRespond`（`command.schema.json#/$defs/elicitationRespond`）的 `payload`：
/// `{ interactionId, action, values }`，其中 `values` 的形状由 `action` 决定（`submit` 为 object
/// 或 `null`，`decline`/`cancel` 为 `null`）。
///
/// `values` 是原始字节保真的开放对象（与 `error.body.details` 同类），因为它的形状由被回答的
/// `elicitation.requested` 决定，本层不做 schema 校验。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ElicitationRespond {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    pub action: ElicitationAction,
    pub values: Nullable<RawObject>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawElicitationRespond {
    #[serde(rename = "interactionId")]
    interaction_id: Uuid,
    action: ElicitationAction,
    values: Box<RawValue>,
}

impl<'de> Deserialize<'de> for ElicitationRespond {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        let fields: RawElicitationRespond =
            serde_json::from_str(raw.get()).map_err(DeError::custom)?;

        // schema 的三个分支不共享 `values` 的取值域：`submit` 是 object 或 null、
        // `decline`/`cancel` 只能是 null。判别字段是 `action`，所以这里必须交叉校验，
        // 不能各解各的。
        let values = match fields.action {
            ElicitationAction::Submit => {
                if is_json_null(&fields.values) {
                    Nullable::null()
                } else {
                    let object = RawObject::parse(fields.values.get()).map_err(|_| {
                        DeError::custom(ValueError::Shape {
                            field: "values",
                            expected: "object | null（action=submit）",
                        })
                    })?;
                    Nullable::value(object)
                }
            }
            ElicitationAction::Decline => {
                if !is_json_null(&fields.values) {
                    return Err(DeError::custom(ValueError::Shape {
                        field: "values",
                        expected: "null（action=decline）",
                    }));
                }
                Nullable::null()
            }
            ElicitationAction::Cancel => {
                if !is_json_null(&fields.values) {
                    return Err(DeError::custom(ValueError::Shape {
                        field: "values",
                        expected: "null（action=cancel）",
                    }));
                }
                Nullable::null()
            }
        };

        Ok(Self {
            interaction_id: fields.interaction_id,
            action: fields.action,
            values,
        })
    }
}

/// `commandStatus`（`command.schema.json#/$defs/commandStatus`）的 `payload`：
/// `{ targetRequestId }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandStatus {
    #[serde(rename = "targetRequestId")]
    pub target_request_id: RequestId,
}

/// `command`（`command.schema.json#/$defs/command`）`body` 的 `payload`（该 def 的 `body.oneOf`）。
///
/// 变体顺序与 schema 的 `oneOf` 顺序一致。payload 里没有自己的判别字段，所以本枚举**没有**
/// `Deserialize`：判别只能由 [`Command::command`] 给出，见 [`CommandPayload::from_raw`]。
///
/// `untagged` 是必须的：payload 直接是命令对象的 `payload` 值，不能带变体名（默认的 externally
/// tagged 编码会写成 `{"SessionPrompt": …}`，与 schema 不符）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum CommandPayload {
    /// `session.list` 的 payload。
    SessionList(SessionList),
    /// `session.read` 的 payload。
    SessionRead(SessionRead),
    /// `session.prompt` 的 payload。
    SessionPrompt(SessionPrompt),
    /// `session.cancel` 的 payload。
    SessionCancel(SessionCancel),
    /// `session.config.list` 的 payload。
    ConfigList(ConfigList),
    /// `session.config.set` 的 payload。
    ConfigSet(ConfigSet),
    /// `session.mode.list` 的 payload。
    ModeList(ModeList),
    /// `session.mode.set` 的 payload。
    ModeSet(ModeSet),
    /// `permission.resolve` 的 payload。
    PermissionResolve(PermissionResolve),
    /// `elicitation.respond` 的 payload。
    ElicitationRespond(ElicitationRespond),
    /// `command.status` 的 payload。
    CommandStatus(CommandStatus),
}

impl CommandPayload {
    /// 按 `command` 的判别把 payload 原文解析成对应变体。
    ///
    /// 形状不符（例如 `command` 说 `session.prompt` 而 payload 是 `session.config.set` 的形状）报
    /// [`ValueError::Shape`]：不会退回开放对象，也不会按其中一个解释后放行。
    fn from_raw(command: CommandName, payload: &RawValue) -> Result<Self, ValueError> {
        match command {
            CommandName::SessionList => {
                parse_payload(payload, "空 object（sessionList 的 payload）")
                    .map(CommandPayload::SessionList)
            }
            CommandName::SessionRead => {
                parse_payload(payload, "{ include: 资源名数组 }（sessionRead 的 payload）")
                    .map(CommandPayload::SessionRead)
            }
            CommandName::SessionPrompt => parse_payload(
                payload,
                "{ content: PromptContentBlock[] }（sessionPrompt 的 payload）",
            )
            .map(CommandPayload::SessionPrompt),
            CommandName::SessionCancel => {
                parse_payload(payload, "{ turnId: uuid }（sessionCancel 的 payload）")
                    .map(CommandPayload::SessionCancel)
            }
            CommandName::SessionConfigList => {
                parse_payload(payload, "空 object（configList 的 payload）")
                    .map(CommandPayload::ConfigList)
            }
            CommandName::SessionConfigSet => parse_payload(
                payload,
                "{ configId: string, value: string|boolean }（configSet 的 payload）",
            )
            .map(CommandPayload::ConfigSet),
            CommandName::SessionModeList => {
                parse_payload(payload, "空 object（modeList 的 payload）")
                    .map(CommandPayload::ModeList)
            }
            CommandName::SessionModeSet => {
                parse_payload(payload, "{ modeId: string }（modeSet 的 payload）")
                    .map(CommandPayload::ModeSet)
            }
            CommandName::PermissionResolve => parse_payload(
                payload,
                "{ interactionId: uuid, optionId: string }（permissionResolve 的 payload）",
            )
            .map(CommandPayload::PermissionResolve),
            CommandName::ElicitationRespond => parse_payload(
                payload,
                "{ interactionId, action, values }（elicitationRespond 的 payload）",
            )
            .map(CommandPayload::ElicitationRespond),
            CommandName::CommandStatus => parse_payload(
                payload,
                "{ targetRequestId: uuid }（commandStatus 的 payload）",
            )
            .map(CommandPayload::CommandStatus),
        }
    }
}

/// `command` 消息的 body（`command.schema.json#/$defs/command` 的 `body`，其 `oneOf` 的 11 个 def）。
///
/// `sessionId` 与 `expectedVersion` 是**条件**字段：哪些命令必须携带、哪些命令禁止携带由 schema
/// 的 `required`/`additionalProperties: false` 决定（[`CommandName::requires_session_id`] /
/// [`CommandName::requires_expected_version`]）。因此它们不是 `Nullable<T>`（那会要求键必须存在，
/// `session.list` 就没有这个键），而是 `Option<T>` + 反序列化期的出现性校验；两个键都不可为 `null`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Command {
    #[serde(rename = "requestId")]
    pub request_id: RequestId,
    pub command: CommandName,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    #[serde(rename = "expectedVersion", skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<DecimalString>,
    pub payload: CommandPayload,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommand {
    #[serde(rename = "requestId")]
    request_id: RequestId,
    command: CommandName,
    #[serde(rename = "sessionId")]
    session_id: Option<Uuid>,
    #[serde(rename = "expectedVersion")]
    expected_version: Option<DecimalString>,
    payload: Box<RawValue>,
}

impl<'de> Deserialize<'de> for Command {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        let fields: RawCommand = serde_json::from_str(raw.get()).map_err(DeError::custom)?;

        // `Option<T>` 把"键缺失"与"值为 null"都变成 `None`，而 schema 用 `additionalProperties:
        // false` 把"键是否允许出现"本身写成了形状的一部分（例如 `session.list` 不得出现
        // `sessionId`），所以键集合要单独看一次（`IgnoredAny` 只收键名，不物化 payload）。
        let present: HashMap<String, IgnoredAny> =
            serde_json::from_str(raw.get()).map_err(DeError::custom)?;

        let payload =
            CommandPayload::from_raw(fields.command, &fields.payload).map_err(DeError::custom)?;
        let command = Self {
            request_id: fields.request_id,
            command: fields.command,
            session_id: fields.session_id,
            expected_version: fields.expected_version,
            payload,
        };
        command
            .validate_presence(
                present.contains_key("sessionId"),
                present.contains_key("expectedVersion"),
            )
            .map_err(DeError::custom)?;
        Ok(command)
    }
}

impl Command {
    /// `sessionId`/`expectedVersion` 的出现规则：必需的命令必须出现，其余命令不得出现
    /// （`additionalProperties: false`），两个键都不接受 `null`。
    fn validate_presence(
        &self,
        session_id_present: bool,
        expected_version_present: bool,
    ) -> Result<(), ValueError> {
        let session_id_required = self.command.requires_session_id();
        if session_id_required && !session_id_present {
            return Err(ValueError::Shape {
                field: "sessionId",
                expected: "该命令的 body 必须携带 sessionId",
            });
        }
        if !session_id_required && session_id_present {
            return Err(ValueError::Shape {
                field: "sessionId",
                expected: "该命令的 body 不得出现 sessionId",
            });
        }
        if session_id_present && self.session_id.is_none() {
            return Err(ValueError::Shape {
                field: "sessionId",
                expected: "uuid（不得为 null）",
            });
        }

        let version_required = self.command.requires_expected_version();
        if version_required && !expected_version_present {
            return Err(ValueError::Shape {
                field: "expectedVersion",
                expected: "该命令的 body 必须携带 expectedVersion（decimalString）",
            });
        }
        if !version_required && expected_version_present {
            return Err(ValueError::Shape {
                field: "expectedVersion",
                expected: "该命令的 body 不得出现 expectedVersion",
            });
        }
        if expected_version_present && self.expected_version.is_none() {
            return Err(ValueError::Shape {
                field: "expectedVersion",
                expected: "decimalString（不得为 null）",
            });
        }

        Ok(())
    }
}

/// `sessionListResult`（`command.schema.json#/$defs/sessionListResult`）：`session.list` 的完成结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionListResult {
    pub sessions: Vec<SessionSummary>,
}

/// `sessionReadResult.resources`：字段与 `sync.snapshotChunk` 的同名资源一致（schema 直接引用
/// `sync.schema.json#/$defs/snapshotItem.*`）。每个键都可缺失，但出现时必须是数组（不可为 `null`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionReadResources {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub messages: Option<Vec<SnapshotItemMessage>>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub turns: Option<Vec<SnapshotItemTurn>>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub pending_interactions: Option<Vec<SnapshotItemPendingInteraction>>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub config_options: Option<Vec<ConfigOptionView>>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub capabilities: Option<Vec<SnapshotItemCapability>>,
}

/// `sessionReadResult`（`command.schema.json#/$defs/sessionReadResult`）：`session.read` 的完成结果。
///
/// 非本端导入的会话由 Owner 回源（`docs/SYNC_PROTOCOL.md` §11.5），wire 形状不受影响。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionReadResult {
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub resources: SessionReadResources,
}

/// `configListResult`（`command.schema.json#/$defs/configListResult`）：`session.config.list` 的
/// 完成结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigListResult {
    #[serde(rename = "configOptions")]
    pub config_options: Vec<ConfigOptionView>,
    pub version: DecimalString,
}

/// `modeListResult`（`command.schema.json#/$defs/modeListResult`）：该 def 是
/// `common.schema.json#/$defs/modeState` 的 `$ref`，因此本类型就是 [`ModeState`]。
pub type ModeListResult = ModeState;

/// `commandStatusRecord`（`command.schema.json#/$defs/commandStatusRecord`）：`command.status` 的
/// 完成结果，也是 `command.status` 的幂等查询结果。
///
/// 字段语义见 `docs/SYNC_PROTOCOL.md` §11.4；schema 只约束 `required` 与各字段类型，因此这里
/// 也只做这些（`state` 与各字段取值的联动是服务端语义，不在 wire 形状里）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandStatusRecord {
    #[serde(rename = "targetRequestId")]
    pub target_request_id: RequestId,
    pub state: CommandState,
    #[serde(rename = "acceptedAt")]
    pub accepted_at: Nullable<Timestamp>,
    #[serde(rename = "terminalAt")]
    pub terminal_at: Nullable<Timestamp>,
    #[serde(rename = "terminalEventId")]
    pub terminal_event_id: Nullable<Uuid>,
    pub result: Nullable<RawObject>,
    pub error: Nullable<PublicError>,
}

/// `commandResult.body.result`：查询命令的 `completed` 由 `command` 决定形状（schema 的
/// `allOf/if-then`），其余情况只有基类型 `object | null`。
///
/// 变体顺序与 schema 的 `allOf` 顺序一致。`result` 的值就是结果对象本身，因此必须 `untagged`
/// （默认的 externally tagged 编码会写成 `{"SessionList": …}`，与 schema 不符）；判别由
/// [`CommandResult::command`] 与 [`CommandResult::status`] 给出，见 [`CommandResultPayload::from_raw`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum CommandResultPayload {
    /// `session.list` 的完成结果。
    SessionList(SessionListResult),
    /// `session.read` 的完成结果。
    SessionRead(SessionReadResult),
    /// `session.config.list` 的完成结果。
    ConfigList(ConfigListResult),
    /// `session.mode.list` 的完成结果。
    ModeList(ModeListResult),
    /// `command.status` 的完成结果。
    CommandStatus(CommandStatusRecord),
    /// 变更命令的完成结果，或非完成态的开放结果对象（基类型 `object | null`）。
    Object(RawObject),
}

impl CommandResultPayload {
    /// 按 `command` 与 `status` 解析 `result` 原文。
    ///
    /// 查询命令的 `completed` 必须落在自己的结果形状上（否则报 [`ValueError::Shape`]，
    /// 例如 `command` 说 `session.list` 而 `result` 是 `configListResult` 的形状）；其余情况只要求
    /// `object | null`。
    fn from_raw(
        command: CommandName,
        status: CommandState,
        result: &RawValue,
    ) -> Result<Nullable<Self>, ValueError> {
        if status == CommandState::Completed {
            match command {
                CommandName::SessionList => {
                    return parse_result::<SessionListResult>(
                        result,
                        "sessionListResult（session.list 的 completed）",
                    )
                    .map(|value| Nullable::value(CommandResultPayload::SessionList(value)));
                }
                CommandName::SessionRead => {
                    return parse_result::<SessionReadResult>(
                        result,
                        "sessionReadResult（session.read 的 completed）",
                    )
                    .map(|value| Nullable::value(CommandResultPayload::SessionRead(value)));
                }
                CommandName::SessionConfigList => {
                    return parse_result::<ConfigListResult>(
                        result,
                        "configListResult（session.config.list 的 completed）",
                    )
                    .map(|value| Nullable::value(CommandResultPayload::ConfigList(value)));
                }
                CommandName::SessionModeList => {
                    return parse_result::<ModeListResult>(
                        result,
                        "modeListResult（session.mode.list 的 completed）",
                    )
                    .map(|value| Nullable::value(CommandResultPayload::ModeList(value)));
                }
                CommandName::CommandStatus => {
                    return parse_result::<CommandStatusRecord>(
                        result,
                        "commandStatusRecord（command.status 的 completed）",
                    )
                    .map(|value| Nullable::value(CommandResultPayload::CommandStatus(value)));
                }
                _ => {}
            }
        }

        if is_json_null(result) {
            return Ok(Nullable::null());
        }
        let object = RawObject::parse(result.get()).map_err(|_| ValueError::Shape {
            field: "result",
            expected: "object 或 null",
        })?;
        Ok(Nullable::value(CommandResultPayload::Object(object)))
    }
}

/// `commandResult`（`command.schema.json#/$defs/commandResult`）的 `body`。
///
/// 七个键全部 required；`acceptedAt`/`terminalEventId`/`result`/`error` 是 `required` 且可 `null`，
/// 因此用 [`Nullable`]（键必须存在）。四个字段的取值由 `command` 与 `status` 共同决定，见
/// [`CommandResult::validate`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandResult {
    #[serde(rename = "requestId")]
    pub request_id: RequestId,
    pub command: CommandName,
    pub status: CommandState,
    #[serde(rename = "acceptedAt")]
    pub accepted_at: Nullable<Timestamp>,
    #[serde(rename = "terminalEventId")]
    pub terminal_event_id: Nullable<Uuid>,
    pub result: Nullable<CommandResultPayload>,
    pub error: Nullable<PublicError>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommandResult {
    #[serde(rename = "requestId")]
    request_id: RequestId,
    command: CommandName,
    status: CommandState,
    #[serde(rename = "acceptedAt")]
    accepted_at: Nullable<Timestamp>,
    #[serde(rename = "terminalEventId")]
    terminal_event_id: Nullable<Uuid>,
    result: Box<RawValue>,
    error: Nullable<PublicError>,
}

impl<'de> Deserialize<'de> for CommandResult {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        let fields: RawCommandResult = serde_json::from_str(raw.get()).map_err(DeError::custom)?;

        let result = CommandResultPayload::from_raw(fields.command, fields.status, &fields.result)
            .map_err(DeError::custom)?;
        let command_result = Self {
            request_id: fields.request_id,
            command: fields.command,
            status: fields.status,
            accepted_at: fields.accepted_at,
            terminal_event_id: fields.terminal_event_id,
            result,
            error: fields.error,
        };
        command_result.validate().map_err(DeError::custom)?;
        Ok(command_result)
    }
}

impl CommandResult {
    /// `command.result.body` 的 `allOf/if-then` 规则（`docs/SYNC_PROTOCOL.md` §11.3）。
    ///
    /// `result` 的形状已由 [`CommandResultPayload::from_raw`] 按 `command`+`status` 处理，这里只查
    /// 其余三个字段。schema 没有约束 `completed` 的 `acceptedAt`（§11.3 另有一句"只有 `rejected`
    /// 才是 `null`"的语义要求），这里只执行 schema 能判定的部分。
    fn validate(&self) -> Result<(), ValueError> {
        match self.status {
            CommandState::Accepted => {
                require_value(
                    self.accepted_at.as_ref(),
                    "acceptedAt",
                    "status=accepted 时必须是时间戳",
                )?;
                require_null(
                    self.terminal_event_id.as_ref(),
                    "terminalEventId",
                    "status=accepted 时必须为 null",
                )?;
                require_null(
                    self.error.as_ref(),
                    "error",
                    "status=accepted 时必须为 null",
                )?;
            }
            CommandState::Completed => {
                if self.command.is_query() {
                    require_null(
                        self.terminal_event_id.as_ref(),
                        "terminalEventId",
                        "查询命令的 completed 必须为 null",
                    )?;
                } else {
                    require_value(
                        self.terminal_event_id.as_ref(),
                        "terminalEventId",
                        "mutation 的 completed 必须给出唯一的 command terminal event id",
                    )?;
                }
            }
            CommandState::Failed | CommandState::Uncertain => {
                require_value(
                    self.accepted_at.as_ref(),
                    "acceptedAt",
                    "status=failed/uncertain 时必须是时间戳",
                )?;
                require_value(
                    self.terminal_event_id.as_ref(),
                    "terminalEventId",
                    "status=failed/uncertain 时必须给出 eventId",
                )?;
            }
            CommandState::Rejected => {
                require_null(
                    self.accepted_at.as_ref(),
                    "acceptedAt",
                    "status=rejected 时必须为 null",
                )?;
                require_null(
                    self.terminal_event_id.as_ref(),
                    "terminalEventId",
                    "status=rejected 时必须为 null",
                )?;
                if !self.result.is_null() {
                    return Err(ValueError::Shape {
                        field: "result",
                        expected: "status=rejected 时必须为 null",
                    });
                }
                require_value(
                    self.error.as_ref(),
                    "error",
                    "status=rejected 时必须携带结构化 error",
                )?;
            }
        }
        Ok(())
    }
}

/// `field` 不得为 `null`。
fn require_value<T>(
    value: Option<&T>,
    field: &'static str,
    expected: &'static str,
) -> Result<(), ValueError> {
    if value.is_none() {
        return Err(ValueError::Shape { field, expected });
    }
    Ok(())
}

/// `field` 必须为 `null`。
fn require_null<T>(
    value: Option<&T>,
    field: &'static str,
    expected: &'static str,
) -> Result<(), ValueError> {
    if value.is_some() {
        return Err(ValueError::Shape { field, expected });
    }
    Ok(())
}

/// 把 payload/result 原文按指定形状解析；形状不符时报 [`ValueError::Shape`]。
fn parse_payload<T: DeserializeOwned>(
    payload: &RawValue,
    expected: &'static str,
) -> Result<T, ValueError> {
    serde_json::from_str(payload.get()).map_err(|_| ValueError::Shape {
        field: "payload",
        expected,
    })
}

/// 与 [`parse_payload`] 相同，但失败落在 `result` 上。
fn parse_result<T: DeserializeOwned>(
    result: &RawValue,
    expected: &'static str,
) -> Result<T, ValueError> {
    serde_json::from_str(result.get()).map_err(|_| ValueError::Shape {
        field: "result",
        expected,
    })
}

/// `RawValue` 的字面量是否为 JSON `null`。
fn is_json_null(raw: &RawValue) -> bool {
    raw.get().trim() == "null"
}
