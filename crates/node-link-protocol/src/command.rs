//! `command` 家族的 body（`schemas/node-link/v1/command.schema.json`、
//! `docs/NODE_LINK_PROTOCOL.md` §12.5/§12.7）。
//!
//! 信封（`protocolVersion`/`type`/`messageId`/`connectionId`/`connectionSequence`）由 `crate::envelope`
//! 承载；本模块只承载 body，与 handshake/catalog/resource/error 四个家族一致。
//!
//! 命令名的唯一机器来源是 `compatibility/commands/v1/commands.json`：[`CommandName`] 是它 `transport`
//! 含 `node_link` 的 12 条命令的镜像（顺序与该 registry 一致，即 Sync 的 11 条之后多出只经 Node Link
//! 接受的 `session.create`）。
//!
//! 校验只发生在反序列化，且只执行 schema 能判定的结构：
//!
//! - `required` 与 `additionalProperties: false`，包括"哪些命令必须携带 `sessionRef`/`attachmentId`/
//!   `attachmentGeneration`/`expectedVersion`、哪些命令必须把同一个字段写成 `null`"这类键与取值同时受限
//!   的规则（`submitBase` 的每个 `allOf` 分支各钉死一组）；
//! - `command` 名与 `payload` 形状一一对应：每个 `submit*` def 把自己的 `command` 钉成一个 `const`，
//!   判别式不符一律报 [`ValueError::Shape`]，不退回开放对象、不按其中一个解释；
//! - `command.accepted`/`command.terminal` 的 `allOf/if-then`：`session.create` 的 accepted 不携带结果
//!   （`result` 必须是 `null`），`completed` 终态必须携带结果，`session.create` 的 `completed` 结果必须是
//!   [`SessionCreateResult`]；`status` 不是 `completed` 时必须给出非 `null` 的 [`PublicError`]。
//!
//! `Deserialize` 返回 `Ok` 即表示该 body 满足 schema。本层**不**校验 schema 也不表达的东西：
//!
//! - `session.create` 的 `cwd`/`mcpServers`/绝对路径/凭据字段由 `additionalProperties: false` 拒绝
//!   （未知键是形状错误），因此不需要白名单；
//! - `templateParams` 的键是否属于该 Export 声明的 template 由 Owner 在应用前校验（§12.7），本层只校验
//!   它是 object 且顶层键数 ≤ 32；
//! - `attachmentGeneration` 是否过期、重复提交的幂等性、`expectedVersion` 冲突判定都是会话状态机。

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use serde::de::{DeserializeOwned, Error as DeError, IgnoredAny};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::common::{
    AgentId, DecimalString, ExportId, NonEmptyText, Nullable, PublicError, RawObject,
    RemoteSessionRef, SessionCreateResult, Timestamp, Uuid, ValueError, WorkspaceAlias,
    deserialize_optional_non_null,
};

/// `requestId`（`command.schema.json#/$defs/requestId`）：该 def 是 `common.schema.json#/$defs/uuid`
/// 的 `$ref`，因此本类型就是 [`Uuid`]。
///
/// 命令的 `requestId` 同时是幂等键（§12.5）：同一 `(ownerNodeId, accessNodeId, requestId)` 的重复提交
/// 必须返回首次结果，`command`/`sessionRef`/`expectedVersion` 或解码后的 `payload` 语义不同则报
/// `nodelink.command.idempotency_conflict`。
pub type RequestId = Uuid;

/// `commandName`（`command.schema.json#/$defs/commandName`）。
///
/// 逐条等于 `compatibility/commands/v1/commands.json` 中 `transport` 含 `node_link` 的 12 条命令，顺序
/// 与该 registry 一致（也是 `docs/NODE_LINK_PROTOCOL.md` §12.5 引用的那 12 个命令名）。
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
    /// `session.create`
    SessionCreate,
}

impl CommandName {
    /// 与 `compatibility/commands/v1/commands.json` 的 node_link 子集逐条相等，顺序一致。
    pub const ALL: [CommandName; 12] = [
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
        CommandName::SessionCreate,
    ];

    /// body 顶层是否**必须**携带 `sessionRef`/`attachmentId`/`attachmentGeneration`。
    ///
    /// 十二个 `submit*` def 把这三个字段绑成一组：会话范围命令三者都必须是 `$ref` 指向的非 `null` 值，
    /// 其余命令（`session.list`、`command.status`、`session.create`）三者都必须显式为 `null`
    /// （§12.5："在不适用时显式写 `null`，不用省略代替"）。
    pub fn requires_session_attachment(self) -> bool {
        !matches!(
            self,
            CommandName::SessionList | CommandName::CommandStatus | CommandName::SessionCreate
        )
    }

    /// body 顶层是否**必须**携带 `expectedVersion`：只有 `session.mode.set` 与 `session.config.set`
    /// 两个乐观并发变更携带，其余命令必须写成 `null`（§12.7 的表格）。
    pub fn requires_expected_version(self) -> bool {
        matches!(
            self,
            CommandName::SessionModeSet | CommandName::SessionConfigSet
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
            CommandName::SessionCreate => "session.create",
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

/// `promptContentBlock.type`：v1 只有 `text` 一个取值（schema 的 `const`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PromptContentType;

impl PromptContentType {
    pub const VALUE: &'static str = "text";
}

impl Serialize for PromptContentType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(Self::VALUE)
    }
}

impl<'de> Deserialize<'de> for PromptContentType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text != Self::VALUE {
            return Err(DeError::custom(ValueError::Enumerated {
                field: "type",
                value: text,
            }));
        }
        Ok(Self)
    }
}

/// `promptContentBlock`（`command.schema.json#/$defs/promptContentBlock`）：一个文本内容块。
///
/// v1 的 prompt 只接受文本：schema 用 `additionalProperties: false` + `const "text"` 把别的 ACP 内容块
/// 排除在外，因此这里不需要"未知块类型"的分支。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptContentBlock {
    #[serde(rename = "type")]
    pub content_type: PromptContentType,
    pub text: NonEmptyText<262144>,
}

/// `session.prompt.payload.content`：`1..=64` 个 [`PromptContentBlock`]（schema 的
/// `minItems`/`maxItems`）。
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
                expected: "至少 1 个 promptContentBlock（schema 的 minItems: 1）",
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

/// `session.read.payload.include`：非空、去重的资源名列表（schema 的 `minItems: 1` 与
/// `uniqueItems: true`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeList(Vec<IncludeResource>);

impl IncludeList {
    pub fn as_slice(&self) -> &[IncludeResource] {
        &self.0
    }

    fn validate(&self) -> Result<(), ValueError> {
        if self.0.is_empty() {
            return Err(ValueError::Shape {
                field: "include",
                expected: "至少 1 个资源名（schema 的 minItems: 1）",
            });
        }
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

/// `configValue`（`command.schema.json#/$defs/configValue`）：`string`（1..=256）或 `boolean`。
///
/// 用哪一个由该 config option 的 `type` 决定（会话状态校验，§12.7），wire 上两者都合法。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ConfigValue {
    /// `string`，1..=256 字符。
    String(NonEmptyText<256>),
    /// `boolean`。
    Boolean(bool),
}

impl<'de> Deserialize<'de> for ConfigValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        if raw.get().trim_start().starts_with('"') {
            return serde_json::from_str::<NonEmptyText<256>>(raw.get())
                .map(ConfigValue::String)
                .map_err(DeError::custom);
        }
        if let Ok(boolean) = serde_json::from_str::<bool>(raw.get()) {
            return Ok(ConfigValue::Boolean(boolean));
        }
        Err(DeError::custom(ValueError::Shape {
            field: "value",
            expected: "string（1..=256）或 boolean（configValue）",
        }))
    }
}

/// `sessionCreatePayload`（`command.schema.json#/$defs/sessionCreatePayload`）的 `templateParams`。
///
/// 开放扩展点：schema 只要求 `type: object` + `maxProperties: 32` + `additionalProperties: true`，
/// 键必须来自该 Export 声明的 template 由 Owner 在应用前校验（§12.7），未知键以
/// `nodelink.command.unsupported_field` 拒绝——那是状态机，不在本层。
///
/// 取值原文按 [`RawObject`] 保真（不重新解析再序列化）；这里只在反序列化期数一次顶层键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateParams(RawObject);

/// `templateParams` 的 `maxProperties`。
const MAX_TEMPLATE_PARAMS: usize = 32;

impl TemplateParams {
    /// 原始 JSON 对象（键与值的字节原样）。
    pub fn as_object(&self) -> &RawObject {
        &self.0
    }
}

impl Serialize for TemplateParams {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TemplateParams {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        // 只收键名（`IgnoredAny` 不物化值），JSON object 之外的字面量在这里就会失败。
        let keys: HashMap<String, IgnoredAny> = serde_json::from_str(raw.get()).map_err(|_| {
            DeError::custom(ValueError::Shape {
                field: "templateParams",
                expected: "object（键必须来自 Export template 声明）",
            })
        })?;
        if keys.len() > MAX_TEMPLATE_PARAMS {
            return Err(DeError::custom(ValueError::TooManyItems {
                field: "templateParams",
                max: MAX_TEMPLATE_PARAMS,
                actual: keys.len(),
            }));
        }
        let object = RawObject::parse(raw.get()).map_err(DeError::custom)?;
        Ok(Self(object))
    }
}

/// `sessionList`（`command.schema.json#/$defs/submitSessionList`）的 `payload`：schema 要求空 object
/// （§12.7 的 `{}`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionList {}

/// `sessionRead`（`command.schema.json#/$defs/submitSessionRead`）的 `payload`：`{ include }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRead {
    pub include: IncludeList,
}

/// `submitCommandStatus`（`command.schema.json#/$defs/submitCommandStatus`）的 `payload`：
/// `{ targetRequestId }`。
///
/// `command.status` 还有独立消息形式（同文件的 `$defs/commandStatus`），两者的 body 形状逐字段相同
/// （§12.5："两者产生相同的 `command.terminal` 形状回复"），因此共用一个类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandStatus {
    #[serde(rename = "targetRequestId")]
    pub target_request_id: RequestId,
}

/// `session.mode.list` 的 `payload`：schema 要求空 object。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionModeList {}

/// `session.config.list` 的 `payload`：schema 要求空 object。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionConfigList {}

/// `sessionPrompt`（`command.schema.json#/$defs/submitSessionPrompt`）的 `payload`：`{ content }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPrompt {
    pub content: PromptContentList,
}

/// `sessionCancel`（`command.schema.json#/$defs/submitSessionCancel`）的 `payload`：`{ turnId }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCancel {
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
}

/// `elicitation.respond.payload.action`（schema 三个分支的 `const`）。
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

/// `submitElicitationRespond`（`command.schema.json#/$defs/submitElicitationRespond`）的 `payload`：
/// `{ interactionId, action, values }`，其中 `values` 的形状由 `action` 决定（`submit` 为 object
/// 或 `null`、`decline`/`cancel` 为 `null`）。
///
/// `values` 是原始字节保真的开放对象（与 `error.body.details` 同类）：它的形状由被回答的那次
/// elicitation 的 schema 决定，本层不做校验。
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

/// `sessionModeSet`（`command.schema.json#/$defs/submitSessionModeSet`）的 `payload`：`{ modeId }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionModeSet {
    #[serde(rename = "modeId")]
    pub mode_id: NonEmptyText<256>,
}

/// `sessionConfigSet`（`command.schema.json#/$defs/submitSessionConfigSet`）的 `payload`：
/// `{ configId, value }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionConfigSet {
    #[serde(rename = "configId")]
    pub config_id: NonEmptyText<256>,
    pub value: ConfigValue,
}

/// `permissionResolve`（`command.schema.json#/$defs/submitPermissionResolve`）的 `payload`：
/// `{ interactionId, optionId }`。
///
/// `optionId` 必须来自该 permission 请求给出的选项集合、`interaction.already_resolved` 之后到达的
/// 应答必须被拒绝——那是 Owner 侧状态机（§15），不在本层。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionResolve {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    #[serde(rename = "optionId")]
    pub option_id: NonEmptyText<128>,
}

/// `session.create` 的 `payload`（`command.schema.json#/$defs/sessionCreatePayload`）：只允许这四个键。
///
/// `cwd`/`mcpServers`/绝对路径/凭据字段由 `additionalProperties: false` 在这里就被拒（未知键），
/// Owner 侧再以 `nodelink.command.unsupported_field` 回复 `command.rejected` 并保证不创建会话、
/// 不部分应用参数（§12.7）。
///
/// `workspace_alias` 是符号名而不是路径：它的 pattern 让绝对路径在语法上就无法通过。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCreate {
    #[serde(rename = "agentId")]
    pub agent_id: AgentId,
    #[serde(rename = "exportId")]
    pub export_id: ExportId,
    #[serde(rename = "workspaceAlias")]
    pub workspace_alias: WorkspaceAlias,
    #[serde(
        rename = "templateParams",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub template_params: Option<TemplateParams>,
}

/// `commandSubmit.body.payload`（`command.schema.json#/$defs/commandSubmit` 的 `body.oneOf` 12 个分支）。
///
/// 变体顺序与 schema 的 `oneOf` 顺序一致（也就是 [`CommandName::ALL`] 的顺序）。payload 里没有自己的
/// 判别字段，所以本枚举**没有** `Deserialize`：判别只能由 [`CommandSubmit::command`] 给出，见
/// [`CommandPayload::from_raw`]。
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
    /// `command.status` 的 payload。
    CommandStatus(CommandStatus),
    /// `session.mode.list` 的 payload。
    SessionModeList(SessionModeList),
    /// `session.config.list` 的 payload。
    SessionConfigList(SessionConfigList),
    /// `session.prompt` 的 payload。
    SessionPrompt(SessionPrompt),
    /// `session.cancel` 的 payload。
    SessionCancel(SessionCancel),
    /// `elicitation.respond` 的 payload。
    ElicitationRespond(ElicitationRespond),
    /// `session.mode.set` 的 payload。
    SessionModeSet(SessionModeSet),
    /// `session.config.set` 的 payload。
    SessionConfigSet(SessionConfigSet),
    /// `permission.resolve` 的 payload。
    PermissionResolve(PermissionResolve),
    /// `session.create` 的 payload。
    SessionCreate(SessionCreate),
}

impl CommandPayload {
    /// 本 payload 对应的命令名；判别式必须与 [`CommandSubmit::command`] 相等。
    pub fn command_name(&self) -> CommandName {
        match self {
            CommandPayload::SessionList(_) => CommandName::SessionList,
            CommandPayload::SessionRead(_) => CommandName::SessionRead,
            CommandPayload::CommandStatus(_) => CommandName::CommandStatus,
            CommandPayload::SessionModeList(_) => CommandName::SessionModeList,
            CommandPayload::SessionConfigList(_) => CommandName::SessionConfigList,
            CommandPayload::SessionPrompt(_) => CommandName::SessionPrompt,
            CommandPayload::SessionCancel(_) => CommandName::SessionCancel,
            CommandPayload::ElicitationRespond(_) => CommandName::ElicitationRespond,
            CommandPayload::SessionModeSet(_) => CommandName::SessionModeSet,
            CommandPayload::SessionConfigSet(_) => CommandName::SessionConfigSet,
            CommandPayload::PermissionResolve(_) => CommandName::PermissionResolve,
            CommandPayload::SessionCreate(_) => CommandName::SessionCreate,
        }
    }

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
            CommandName::SessionRead => parse_payload(
                payload,
                "{ include: 资源名数组（≥1 且去重）}（sessionRead 的 payload）",
            )
            .map(CommandPayload::SessionRead),
            CommandName::CommandStatus => {
                parse_payload(payload, "{ targetRequestId: uuid }（commandStatus 的 payload）")
                    .map(CommandPayload::CommandStatus)
            }
            CommandName::SessionModeList => {
                parse_payload(payload, "空 object（sessionModeList 的 payload）")
                    .map(CommandPayload::SessionModeList)
            }
            CommandName::SessionConfigList => {
                parse_payload(payload, "空 object（sessionConfigList 的 payload）")
                    .map(CommandPayload::SessionConfigList)
            }
            CommandName::SessionPrompt => parse_payload(
                payload,
                "{ content: promptContentBlock[]（1..=64）}（sessionPrompt 的 payload）",
            )
            .map(CommandPayload::SessionPrompt),
            CommandName::SessionCancel => {
                parse_payload(payload, "{ turnId: uuid }（sessionCancel 的 payload）")
                    .map(CommandPayload::SessionCancel)
            }
            CommandName::ElicitationRespond => parse_payload(
                payload,
                "{ interactionId, action, values }（elicitationRespond 的 payload）",
            )
            .map(CommandPayload::ElicitationRespond),
            CommandName::SessionModeSet => parse_payload(
                payload,
                "{ modeId: string（1..=256）}（sessionModeSet 的 payload）",
            )
            .map(CommandPayload::SessionModeSet),
            CommandName::SessionConfigSet => parse_payload(
                payload,
                "{ configId: string（1..=256）, value: string|boolean }（sessionConfigSet 的 payload）",
            )
            .map(CommandPayload::SessionConfigSet),
            CommandName::PermissionResolve => parse_payload(
                payload,
                "{ interactionId: uuid, optionId: string（1..=128）}（permissionResolve 的 payload）",
            )
            .map(CommandPayload::PermissionResolve),
            CommandName::SessionCreate => parse_payload(
                payload,
                "{ agentId, exportId, workspaceAlias, templateParams? }（sessionCreate 的 payload）",
            )
            .map(CommandPayload::SessionCreate),
        }
    }
}

/// `command.submit` 消息的 body（`command.schema.json#/$defs/commandSubmit` 的 `body`）。
///
/// 七个键全部 required：`sessionRef`/`attachmentId`/`attachmentGeneration`/`expectedVersion` 是
/// required 且可 `null`，因此用 [`Nullable`]（键必须存在，`null` 是"本命令不使用该字段"的显式取值）。
/// 具体的 null/非 null 规则由 [`CommandName::requires_session_attachment`] 与
/// [`CommandName::requires_expected_version`] 决定，见 [`CommandSubmit::validate`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandSubmit {
    #[serde(rename = "requestId")]
    pub request_id: RequestId,
    pub command: CommandName,
    #[serde(rename = "sessionRef")]
    pub session_ref: Nullable<RemoteSessionRef>,
    #[serde(rename = "attachmentId")]
    pub attachment_id: Nullable<Uuid>,
    #[serde(rename = "attachmentGeneration")]
    pub attachment_generation: Nullable<DecimalString>,
    #[serde(rename = "expectedVersion")]
    pub expected_version: Nullable<DecimalString>,
    pub payload: CommandPayload,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommandSubmit {
    #[serde(rename = "requestId")]
    request_id: RequestId,
    command: CommandName,
    #[serde(rename = "sessionRef")]
    session_ref: Nullable<RemoteSessionRef>,
    #[serde(rename = "attachmentId")]
    attachment_id: Nullable<Uuid>,
    #[serde(rename = "attachmentGeneration")]
    attachment_generation: Nullable<DecimalString>,
    #[serde(rename = "expectedVersion")]
    expected_version: Nullable<DecimalString>,
    payload: Box<RawValue>,
}

impl<'de> Deserialize<'de> for CommandSubmit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        let fields: RawCommandSubmit = serde_json::from_str(raw.get()).map_err(DeError::custom)?;

        let payload =
            CommandPayload::from_raw(fields.command, &fields.payload).map_err(DeError::custom)?;
        let submit = Self {
            request_id: fields.request_id,
            command: fields.command,
            session_ref: fields.session_ref,
            attachment_id: fields.attachment_id,
            attachment_generation: fields.attachment_generation,
            expected_version: fields.expected_version,
            payload,
        };
        submit.validate().map_err(DeError::custom)?;
        Ok(submit)
    }
}

impl CommandSubmit {
    /// `submitBase` + 各 `submit*` 分支的字段出现规则：会话范围命令必须携带
    /// `sessionRef`/`attachmentId`/`attachmentGeneration`，`session.mode.set` 与 `session.config.set`
    /// 必须携带 `expectedVersion`，其余位置必须显式为 `null`；`payload` 形状必须与 `command` 对应。
    fn validate(&self) -> Result<(), ValueError> {
        if self.command.requires_session_attachment() {
            require_value(
                self.session_ref.as_ref(),
                "sessionRef",
                "会话范围命令必须携带 remoteSessionRef（不得为 null）",
            )?;
            require_value(
                self.attachment_id.as_ref(),
                "attachmentId",
                "会话范围命令必须携带 attachmentId（uuid，不得为 null）",
            )?;
            require_value(
                self.attachment_generation.as_ref(),
                "attachmentGeneration",
                "会话范围命令必须携带 attachmentGeneration（decimalString，不得为 null）",
            )?;
        } else {
            require_null(
                self.session_ref.as_ref(),
                "sessionRef",
                "该命令没有会话范围，sessionRef 必须为 null",
            )?;
            require_null(
                self.attachment_id.as_ref(),
                "attachmentId",
                "该命令没有会话范围，attachmentId 必须为 null",
            )?;
            require_null(
                self.attachment_generation.as_ref(),
                "attachmentGeneration",
                "该命令没有会话范围，attachmentGeneration 必须为 null",
            )?;
        }

        if self.command.requires_expected_version() {
            require_value(
                self.expected_version.as_ref(),
                "expectedVersion",
                "该命令必须携带 expectedVersion（decimalString，不得为 null）",
            )?;
        } else {
            require_null(
                self.expected_version.as_ref(),
                "expectedVersion",
                "该命令不得携带 expectedVersion（必须为 null）",
            )?;
        }

        // 判别式：`command` 名与 payload 形状一一对应。解码路径上 payload 变体是按 `command` 选出来的，
        // 这里再挡一次手工构造出的不一致（构造出的值必须同样满足 schema）。
        if self.payload.command_name() != self.command {
            return Err(ValueError::Shape {
                field: "payload",
                expected: "payload 形状必须与 command 名一一对应（如 command=session.create 时只能是 sessionCreatePayload）",
            });
        }
        Ok(())
    }
}

/// `command.terminal.body.terminal.status`（schema 的 inline enum）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalStatus {
    /// `completed`：命令已成功终结，`result` 必须携带结果且 `error` 为 `null`。
    Completed,
    /// `failed`：已接受的命令失败。
    Failed,
    /// `rejected`：未产生业务副作用，`error` 携带结构化原因。
    Rejected,
    /// `uncertain`：崩溃窗口内无法确认副作用是否发生；Access 不得自动重试（§12.5/§12.7）。
    Uncertain,
}

impl TerminalStatus {
    /// schema 的 enum 取值，顺序一致。
    pub const ALL: [TerminalStatus; 4] = [
        TerminalStatus::Completed,
        TerminalStatus::Failed,
        TerminalStatus::Rejected,
        TerminalStatus::Uncertain,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TerminalStatus::Completed => "completed",
            TerminalStatus::Failed => "failed",
            TerminalStatus::Rejected => "rejected",
            TerminalStatus::Uncertain => "uncertain",
        }
    }
}

impl fmt::Display for TerminalStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for TerminalStatus {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        TerminalStatus::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "status",
                value: text.to_owned(),
            })
    }
}

impl Serialize for TerminalStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TerminalStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        TerminalStatus::from_str(&text).map_err(DeError::custom)
    }
}

/// `command.accepted.body.result` 与 `command.terminal.body.terminal.result` 的取值。
///
/// 变体顺序与 schema 的 `allOf` 顺序一致。`result` 的值就是结果对象本身，因此必须 `untagged`
/// （默认的 externally tagged 编码会写成 `{"SessionCreate": …}`，与 schema 不符）；判别由命令名与终态
/// 给出，见 [`CommandResultPayload::from_accepted`] / [`CommandResultPayload::from_terminal`]。
///
/// Node Link v1 只给 `session.create` 的成功终态取了名字（`sessionCreateResult`）：其余命令的结果形状
/// 与 Sync 的同名命令相同但引用不到本 schema（`command.schema.json` 的 `result` 只有基类型
/// `object | null`），因此按 [`RawObject`] 字节保真承载，本层不做类型化解释。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum CommandResultPayload {
    /// `session.create` 的 `completed` 结果（§12.7 的结果契约）。
    SessionCreate(SessionCreateResult),
    /// 其余情况的开放结果对象。
    Object(RawObject),
}

impl CommandResultPayload {
    /// `command.accepted.result`：`session.create` 是异步的，accepted 的 `result` 必须为 `null`
    /// （schema 的 `if/then`）；其余命令可以是同步结果对象或 `null`。
    fn from_accepted(
        command: CommandName,
        result: &RawValue,
    ) -> Result<Nullable<Self>, ValueError> {
        if command == CommandName::SessionCreate {
            if !is_json_null(result) {
                return Err(ValueError::Shape {
                    field: "result",
                    expected: "null（session.create 的 command.accepted 不携带结果，结果由 command.terminal 给出）",
                });
            }
            return Ok(Nullable::null());
        }
        open_result_object(result)
    }

    /// `command.terminal.terminal.result`：`session.create` 的 `completed` 必须是
    /// [`SessionCreateResult`]；其余情况是 `object | null`。
    ///
    /// `completed` 必须有结果、其余 status 的 `error` 规则在 [`Terminal::validate`] 里执行。
    fn from_terminal(
        command: CommandName,
        status: TerminalStatus,
        result: &RawValue,
    ) -> Result<Nullable<Self>, ValueError> {
        if status == TerminalStatus::Completed && command == CommandName::SessionCreate {
            return parse_result::<SessionCreateResult>(
                result,
                "sessionCreateResult（session.create 的 completed）",
            )
            .map(|value| Nullable::value(CommandResultPayload::SessionCreate(value)));
        }
        open_result_object(result)
    }
}

/// `result` 为 `object | null`（开放对象）时的解析：`null` → [`Nullable::null`]，其余按键序保真承载。
fn open_result_object(result: &RawValue) -> Result<Nullable<CommandResultPayload>, ValueError> {
    if is_json_null(result) {
        return Ok(Nullable::null());
    }
    let object = RawObject::parse(result.get()).map_err(|_| ValueError::Shape {
        field: "result",
        expected: "object 或 null",
    })?;
    Ok(Nullable::value(CommandResultPayload::Object(object)))
}

/// `command.terminal.body.terminal`：终态本体。
///
/// 五个键全部 required，`terminalEventId`/`result`/`error` 是 required 且可 `null`，因此用 [`Nullable`]。
/// `result` 的形状需要命令名才能判定（`session.create` 的 `completed`），所以本类型**没有**
/// `Deserialize`：解析入口是 [`Terminal::from_raw`]，由 [`CommandTerminal`] 传入 `command`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Terminal {
    pub status: TerminalStatus,
    #[serde(rename = "terminalAt")]
    pub terminal_at: Timestamp,
    /// `command.status` 重查返回时必须非 `null`，其余情况可为 `null`（§12.5）。
    #[serde(rename = "terminalEventId")]
    pub terminal_event_id: Nullable<Uuid>,
    pub result: Nullable<CommandResultPayload>,
    pub error: Nullable<PublicError>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTerminal {
    status: TerminalStatus,
    #[serde(rename = "terminalAt")]
    terminal_at: Timestamp,
    #[serde(rename = "terminalEventId")]
    terminal_event_id: Nullable<Uuid>,
    result: Box<RawValue>,
    error: Nullable<PublicError>,
}

impl Terminal {
    /// 按 `command`（决定 `result` 的形状）解析 `terminal` 原文。
    fn from_raw(command: CommandName, value: &RawValue) -> Result<Self, ValueError> {
        let fields: RawTerminal =
            serde_json::from_str(value.get()).map_err(|_| ValueError::Shape {
                field: "terminal",
                expected: "{ status, terminalAt, terminalEventId, result, error }（commandTerminal 的 terminal）",
            })?;
        let terminal = Self {
            status: fields.status,
            terminal_at: fields.terminal_at,
            terminal_event_id: fields.terminal_event_id,
            result: CommandResultPayload::from_terminal(command, fields.status, &fields.result)?,
            error: fields.error,
        };
        terminal.validate()?;
        Ok(terminal)
    }

    /// `terminal` 的 `allOf/if-then` 规则（§12.5）：`completed` 必须携带结果且 `error` 为 `null`；
    /// `failed`/`rejected`/`uncertain` 必须给出 `error`。
    fn validate(&self) -> Result<(), ValueError> {
        match self.status {
            TerminalStatus::Completed => {
                if self.result.is_null() {
                    return Err(ValueError::Shape {
                        field: "result",
                        expected: "object（status=completed 必须携带结果）",
                    });
                }
                require_null(
                    self.error.as_ref(),
                    "error",
                    "status=completed 时 error 必须为 null",
                )?;
            }
            TerminalStatus::Failed | TerminalStatus::Rejected | TerminalStatus::Uncertain => {
                require_value(
                    self.error.as_ref(),
                    "error",
                    "status=failed/rejected/uncertain 必须给出结构化 error",
                )?;
            }
        }
        Ok(())
    }
}

/// `command.terminal` 消息的 body（`command.schema.json#/$defs/commandTerminal` 的 `body`）。
///
/// `command` 决定 `terminal.result` 的形状（`session.create` 的 `completed` 必须是
/// [`SessionCreateResult`]），因此反序列化必须把两者一起看，不能用「先解 command 再解 terminal」的
/// 两段式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandTerminal {
    #[serde(rename = "requestId")]
    pub request_id: RequestId,
    pub command: CommandName,
    pub terminal: Terminal,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommandTerminal {
    #[serde(rename = "requestId")]
    request_id: RequestId,
    command: CommandName,
    terminal: Box<RawValue>,
}

impl<'de> Deserialize<'de> for CommandTerminal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        let fields: RawCommandTerminal =
            serde_json::from_str(raw.get()).map_err(DeError::custom)?;
        let terminal =
            Terminal::from_raw(fields.command, &fields.terminal).map_err(DeError::custom)?;
        Ok(Self {
            request_id: fields.request_id,
            command: fields.command,
            terminal,
        })
    }
}

/// `command.accepted` 消息的 body（`command.schema.json#/$defs/commandAccepted` 的 `body`）。
///
/// 四个键全部 required；`acceptedAt` 永远是真实的接受时间（拒绝不复用本消息），`result` 为 `null` 时
/// 表示本次回复不携带同步结果。`session.create` 的 `result` 必须是 `null`（见
/// [`CommandResultPayload::from_accepted`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandAccepted {
    #[serde(rename = "requestId")]
    pub request_id: RequestId,
    pub command: CommandName,
    #[serde(rename = "acceptedAt")]
    pub accepted_at: Timestamp,
    pub result: Nullable<CommandResultPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommandAccepted {
    #[serde(rename = "requestId")]
    request_id: RequestId,
    command: CommandName,
    #[serde(rename = "acceptedAt")]
    accepted_at: Timestamp,
    result: Box<RawValue>,
}

impl<'de> Deserialize<'de> for CommandAccepted {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        let fields: RawCommandAccepted =
            serde_json::from_str(raw.get()).map_err(DeError::custom)?;
        let result = CommandResultPayload::from_accepted(fields.command, &fields.result)
            .map_err(DeError::custom)?;
        Ok(Self {
            request_id: fields.request_id,
            command: fields.command,
            accepted_at: fields.accepted_at,
            result,
        })
    }
}

/// `command.rejected` 消息的 body（`command.schema.json#/$defs/commandRejected` 的 `body`）。
///
/// 三个键全部 required，本消息**没有** `acceptedAt`：拒绝表达的是"未接受、无副作用"，与
/// `command.terminal` 的 `status = "rejected"` 一样都靠 [`PublicError`] 给出结构化原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRejected {
    #[serde(rename = "requestId")]
    pub request_id: RequestId,
    pub command: CommandName,
    pub error: PublicError,
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

/// `RawValue` 的字面量是否为 JSON `null`。
fn is_json_null(raw: &RawValue) -> bool {
    raw.get().trim() == "null"
}
