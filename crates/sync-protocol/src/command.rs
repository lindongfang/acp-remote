//! `command` 与 `command.result` 的 body（`schemas/sync/v1/command.schema.json`、
//! `docs/SYNC_PROTOCOL.md` §11）。
//!
//! 信封（`protocolVersion`/`type`/`messageId`/`connectionId`/`connectionSequence`）由
//! [`crate::envelope`] 承载；本模块只承载 `body`，与 `auth`/`control`/`error` 三个家族一致。
//!
//! 命令名的唯一机器来源是 `compatibility/commands/v1/commands.json`：[`CommandName`] 是它
//! `transport` 含 `sync` 的 14 条命令的镜像（`session.resume` 只经 Node Link 接受，Sync v1 收到时
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
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::common::{
    ConfigOptionValue, ConfigOptionView, DecimalString, ModeState, NonEmptyText, Nullable,
    PromptContentBlock, PublicError, RawObject, SessionSummary, Timestamp, UIntAtLeast, Uuid,
    ValueError, deserialize_optional_non_null,
};
use crate::sync::{
    SnapshotItemCapability, SnapshotItemHistory, SnapshotItemMessage,
    SnapshotItemPendingInteraction, SnapshotItemTurn,
};

/// `requestId`（`command.schema.json#/$defs/requestId`）：该 def 是 `common.schema.json#/$defs/uuid`
/// 的 `$ref`，因此本类型就是 [`Uuid`]。
///
/// 命令的 `requestId` 同时是幂等键（`docs/SYNC_PROTOCOL.md` §11.1）：客户端在响应丢失后必须用同一个
/// id 重试，服务端必须在重复提交时返回相同的已知接受/终态。
pub type RequestId = Uuid;

/// `commandName`（`command.schema.json#/$defs/commandName`）。
///
/// 逐条等于 `compatibility/commands/v1/commands.json` 中 `transport` 含 `sync` 的 14 条命令，顺序
/// 与该 registry 一致（也是 `docs/SYNC_PROTOCOL.md` §11.5 首列去掉 `session.resume` 的结果）。
///
/// `session.create` 与 Node Link 面同名，但 payload 不同：Sync 侧是
/// `{ agentId, directoryId | workspacePath（恰好一个）, createIfMissing? }`（`templateParams` 等
/// 跨节点概念不属于 Sync 面，见 `docs/SYNC_PROTOCOL.md` §11.5）。
///
/// `workspace.browse` 只经 Sync 面接受（它是本机目录的逐层浏览查询，Node Link 的请求不得携带任意
/// Owner 路径，因此不在 Node Link 的命令枚举里）。
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
    /// `workspace.browse`
    WorkspaceBrowse,
    /// `workspace.mkdir`
    WorkspaceMkdir,
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
    /// 与 `compatibility/commands/v1/commands.json` 的 sync 子集逐条相等，顺序一致。
    pub const ALL: [CommandName; 14] = [
        CommandName::SessionList,
        CommandName::SessionRead,
        CommandName::CommandStatus,
        CommandName::SessionModeList,
        CommandName::SessionConfigList,
        CommandName::WorkspaceMkdir,
        CommandName::WorkspaceBrowse,
        CommandName::SessionPrompt,
        CommandName::SessionCancel,
        CommandName::ElicitationRespond,
        CommandName::SessionModeSet,
        CommandName::SessionConfigSet,
        CommandName::PermissionResolve,
        CommandName::SessionCreate,
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
                | CommandName::WorkspaceBrowse
        )
    }

    /// body 顶层是否**必须**携带 `sessionId`（各 def 的 `required` 列表；其余命令按
    /// `additionalProperties: false` 反过来禁止它）。`session.create` 的目标会话尚不存在，
    /// `workspace.browse` 只浏览目录而不属于任何会话，因此两者与 `session.list`/`command.status`
    /// 一样禁止该键。
    pub fn requires_session_id(self) -> bool {
        !matches!(
            self,
            CommandName::SessionList
                | CommandName::CommandStatus
                | CommandName::SessionCreate
                | CommandName::WorkspaceBrowse
                | CommandName::WorkspaceMkdir
        )
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
            CommandName::WorkspaceBrowse => "workspace.browse",
            CommandName::WorkspaceMkdir => "workspace.mkdir",
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

/// `workspaceBrowse` 判定父路径与条目路径的长度域（`command.schema.json#/$defs/workspaceBrowsePath`，
/// 与 `workspacePath` 同为 1..=4096）。
pub type WorkspaceBrowsePath = NonEmptyText<4096>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceMkdir {
    pub parent_path: WorkspaceBrowsePath,
    #[serde(deserialize_with = "deserialize_directory_name")]
    pub name: NonEmptyText<255>,
}

fn deserialize_directory_name<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<NonEmptyText<255>, D::Error> {
    let name = NonEmptyText::<255>::deserialize(deserializer)?;
    let text = name.as_str();
    if text == "." || text == ".." || text.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0')) {
        return Err(D::Error::custom("directory name must be one component"));
    }
    Ok(name)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMkdirResult {
    #[serde(deserialize_with = "deserialize_created")]
    pub created: bool,
}

fn deserialize_created<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    if bool::deserialize(deserializer)? {
        Ok(true)
    } else {
        Err(D::Error::custom("created must be true"))
    }
}

/// `workspaceBrowse`（`command.schema.json#/$defs/workspaceBrowse`）的 `payload`：只含一个父路径键，
/// 取值是绝对路径或 `null`（`null` = 该节点的根列表）。形状校验（相对路径、含 `..`）与可达性判定
/// 属于本机用例，不在这一层判定——这一层只保证「恰好一个路径键，且不是空串」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceBrowse {
    pub path: Nullable<WorkspaceBrowsePath>,
}

/// `workspaceBrowseEntry`（`command.schema.json#/$defs/workspaceBrowseEntry`）：该层的**直接**子目录
/// 条目。只有名称、绝对路径与「是否已登记 workspace」三项：文件条目、文件内容、大小、时间戳与权限
/// 都不在 wire 上。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceBrowseEntry {
    pub name: NonEmptyText<256>,
    pub path: WorkspaceBrowsePath,
    /// 该子目录是否已是本机已登记的 workspace（只反映登记状态，不产生任何登记副作用）。
    pub registered: bool,
}

/// `workspaceBrowseResult`（`command.schema.json#/$defs/workspaceBrowseResult`）：`workspace.browse` 的
/// completed 结果——回显的父路径、该层条目与截断标记。
///
/// `truncated` 为真表示该层的子目录数超过固定 v1 上限（`docs/SYNC_PROTOCOL.md` §14），`entries`
/// 因此只含上限内的条目；它不是「结果不完整所以失败」，客户端据此提示还有更多。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceBrowseResult {
    pub path: Nullable<WorkspaceBrowsePath>,
    pub entries: Vec<WorkspaceBrowseEntry>,
    pub truncated: bool,
}

/// `sessionCreate`（`command.schema.json#/$defs/sessionCreate`）的 `payload`。
///
/// schema 要求 `agentId` 必填，且 `directoryId` 与 `workspacePath` **恰好出现一个**（`oneOf`）；
/// 目录来源是 [`SessionCreateWorkspace`] 的二选一，因此"两个都给"或"都不给"在构造与反序列化两侧
/// 都不可能成立。
///
/// [`create_if_missing`](Self::create_if_missing) 是可选的显式「目录不存在则先创建」开关：缺席即
/// 假（行为与引入该键之前完全一致），只有客户端提供的目录才可能与它同时出现——它与
/// `directoryId` **互斥**（已记录的目录必然已存在），同时给出时按 schema 的 `not` 在
/// 反序列化期被拒。
///
/// `directoryId` 对应本机 `owned_directory` 已记录的行，`workspacePath` 是客户端提供的绝对
/// 目录（浏览器拿不到宿主路径，因此这一来源就是一次文本输入），`agentId` 对应本机已配置的 Agent
/// profile。规范化路径、目录追加、MCP 配置与凭据都不在 wire 上，因此 `deny_unknown_fields` 会把
/// `cwd`/`templateParams` 之类的键直接拒成 [`ValueError::Shape`]——与 schema 的
/// `additionalProperties: false` 同口径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCreate {
    pub agent_id: NonEmptyText<128>,
    pub workspace: SessionCreateWorkspace,
    /// 键缺席即默认「不创建」；键在场时只接受 `true`/`false`（没有 `null` 分支）。
    pub create_if_missing: Option<bool>,
}

/// `session.create` 的目录来源：本机已记录的目录标识或客户端提供的绝对路径，**恰好一个**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCreateWorkspace {
    /// `directoryId`：本机已记录目录的 32 字符小写十六进制标识（schema 的 `pattern`）。
    Directory(NonEmptyText<32>),
    /// `workspacePath`：客户端提供的**绝对**路径（schema 的 `maxLength: 4096`）。本机校验它存在、
    /// 是目录，并以 `canonicalize` 的结果为权威值；形状校验与解析失败由本机用例分类，不在本层判定。
    Path(NonEmptyText<4096>),
}

impl Serialize for SessionCreate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("agentId", &self.agent_id)?;
        match &self.workspace {
            SessionCreateWorkspace::Directory(directory_id) => {
                map.serialize_entry("directoryId", directory_id)?
            }
            SessionCreateWorkspace::Path(path) => map.serialize_entry("workspacePath", path)?,
        }
        if let Some(create_if_missing) = self.create_if_missing {
            map.serialize_entry("createIfMissing", &create_if_missing)?;
        }
        map.end()
    }
}

/// [`SessionCreate`] 的反序列化中间形态：两个目录键都声明为可缺失的 `Option`，由
/// [`TryFrom`] 把"恰好一个"这条 `oneOf` 与「建目录开关不与别名同用」这条 `not` 一起钉死。键存在但为
/// `null` 必须被拒（它们不是可空字段），因此用 [`deserialize_optional_non_null`] 而不是裸 `Option`。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSessionCreate {
    #[serde(rename = "agentId")]
    agent_id: NonEmptyText<128>,
    #[serde(
        rename = "directoryId",
        default,
        deserialize_with = "deserialize_optional_non_null"
    )]
    directory_id: Option<NonEmptyText<32>>,
    #[serde(
        rename = "workspacePath",
        default,
        deserialize_with = "deserialize_optional_non_null"
    )]
    workspace_path: Option<NonEmptyText<4096>>,
    /// 建目录开关：键缺席即 `None`（默认假），键在场时只接受 `true`/`false`（没有 `null` 分支）。
    /// 保留「键在不在」这一维，才能与 `directoryId` 互斥（`Some(false)` 也算同时出现）。
    #[serde(
        rename = "createIfMissing",
        default,
        deserialize_with = "deserialize_optional_non_null"
    )]
    create_if_missing: Option<bool>,
}

impl TryFrom<RawSessionCreate> for SessionCreate {
    type Error = ValueError;

    fn try_from(raw: RawSessionCreate) -> Result<Self, Self::Error> {
        let workspace = match (raw.directory_id, raw.workspace_path) {
            (Some(directory_id), None) => SessionCreateWorkspace::Directory(directory_id),
            (None, Some(path)) => SessionCreateWorkspace::Path(path),
            _ => {
                return Err(ValueError::Shape {
                    field: "payload",
                    expected: "directoryId 与 workspacePath 必须恰好给出一个（command.schema.json#/$defs/sessionCreate 的 oneOf）",
                });
            }
        };
        // 已记录的目录必然已存在，因此「不存在则先创建」与它同时出现是形状错误
        // （command.schema.json#/$defs/sessionCreate.payload 的 `not`）；值是不是 true 都算同时出现。
        if matches!(workspace, SessionCreateWorkspace::Directory(_))
            && raw.create_if_missing.is_some()
        {
            return Err(ValueError::Shape {
                field: "payload",
                expected: "createIfMissing 不得与 directoryId 同时出现（command.schema.json#/$defs/sessionCreate.payload 的 not）",
            });
        }
        Ok(Self {
            agent_id: raw.agent_id,
            workspace,
            create_if_missing: raw.create_if_missing,
        })
    }
}

impl<'de> Deserialize<'de> for SessionCreate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawSessionCreate::deserialize(deserializer)?;
        Self::try_from(raw).map_err(DeError::custom)
    }
}

/// `session.read.payload.include` 的元素（schema 的 `items.enum`）。
///
/// 取值与 schema 的 `items.enum` 逐条同序；`History` 是 Sync 面独有的结构化历史条目资源
/// （design D11：Node Link 面的同名命令不认这个取值，其形状不变）。
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
    /// `history`
    History,
}

impl IncludeResource {
    /// schema 的 `items.enum`，顺序一致。
    pub const ALL: [IncludeResource; 6] = [
        IncludeResource::Messages,
        IncludeResource::Turns,
        IncludeResource::PendingInteractions,
        IncludeResource::ConfigOptions,
        IncludeResource::Capabilities,
        IncludeResource::History,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            IncludeResource::Messages => "messages",
            IncludeResource::Turns => "turns",
            IncludeResource::PendingInteractions => "pending_interactions",
            IncludeResource::ConfigOptions => "config_options",
            IncludeResource::Capabilities => "capabilities",
            IncludeResource::History => "history",
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

/// `sessionReadBefore`（`command.schema.json#/$defs/sessionReadBefore`）：`session.read` 的复合分页
/// 游标 `{ createdAt, messageId }`（`docs/SYNC_PROTOCOL.md` §11.5，design D2）。
///
/// 两个键都必需，因此 `deny_unknown_fields` 与「只给一项」共同把单字段游标拒成
/// [`ValueError::Shape`]——服务端不得猜测或补全缺失的排序分量。排序键是 `createdAt` 升序，
/// `messageId` 只作同一时刻内的 tie-breaker；它的数值与字典序都不是排序依据（§3.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionReadBefore {
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    #[serde(rename = "messageId")]
    pub message_id: Uuid,
}

/// `sessionRead`（`command.schema.json#/$defs/sessionRead`）的 `payload`：
/// `{ include, before?, beforeHistory?, limit? }`。
///
/// `before`/`beforeHistory` 与 `limit` 只在 Sync 传输面生效（design D3、D11）：`node-link-protocol`
/// 的同名命令载荷不含这些键，本 crate 不镜像它们。两套游标互不影响：`before` 翻消息页（时间与消息
/// 标识的复合游标），`beforeHistory` 翻结构化历史条目页（会话内事件序号，十进制字符串）。省略任一
/// 游标表示该资源取最新一页；`limit` 省略时服务端用配置默认页，超过服务端上限时服务端收敛到自己的
/// 上限而不报错，因此 schema 侧没有 `maximum`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRead {
    pub include: IncludeList,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<SessionReadBefore>,
    #[serde(
        rename = "beforeHistory",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub before_history: Option<DecimalString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<UIntAtLeast<1>>,
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
    /// `session.create` 的 payload。
    SessionCreate(SessionCreate),
    /// `workspace.browse` 的 payload。
    WorkspaceBrowse(WorkspaceBrowse),
    WorkspaceMkdir(WorkspaceMkdir),
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
            CommandName::SessionCreate => parse_payload(
                payload,
                "{ agentId, directoryId | workspacePath（恰好一个） }（sessionCreate 的 payload）",
            )
            .map(CommandPayload::SessionCreate),
            CommandName::WorkspaceMkdir => parse_payload(payload, "{ parentPath, name }").map(CommandPayload::WorkspaceMkdir),
            CommandName::WorkspaceBrowse => parse_payload(
                payload,
                "{ path: 绝对路径 | null }（workspaceBrowse 的 payload）",
            )
            .map(CommandPayload::WorkspaceBrowse),
            CommandName::SessionRead => parse_payload(
                payload,
                "{ include: 资源名数组, before?: { createdAt, messageId }, beforeHistory?: decimalString, limit?: >=1 }（sessionRead 的 payload）",
            )
            .map(CommandPayload::SessionRead),
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

/// `command` 消息的 body（`command.schema.json#/$defs/command` 的 `body`，其 `oneOf` 的 12 个 def）。
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

/// `sessionCreateResult`（`command.schema.json#/$defs/sessionCreateResult`）：`session.create` 的
/// 完成结果——新会话的标识与它的 [`SessionSummary`]。
///
/// 摘要复用 `session.list`/`session.created` 的同一形状，因此目录引用等可选字段只在本机与对端
/// 协商过对应 feature 时出现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCreateResult {
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub session: SessionSummary,
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
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub history: Option<Vec<SnapshotItemHistory>>,
}

/// `sessionReadResult`（`command.schema.json#/$defs/sessionReadResult`）：`session.read` 的完成结果。
///
/// 非本端导入的会话由 Owner 回源（`docs/SYNC_PROTOCOL.md` §11.5），wire 形状不受影响。
///
/// `has_earlier` 是必填布尔值：本次返回的这一页之前是否仍有更早内容。它为 `false` 时客户端必须停止
/// 翻页，不得把「本页为空」当作「已到最早一条」之外的任何结论。
///
/// [`history_has_earlier`](Self::history_has_earlier) 是可选的、与 `has_earlier` **互相独立**的页标记：
/// 它只描述 `resources.history` 这一页之前是否仍有更早条目（design D2 的序号游标口径），既不改变
/// `has_earlier` 的消息语义，也不被 `has_earlier` 取代。缺席时表示本次未请求 `history`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionReadResult {
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub resources: SessionReadResources,
    #[serde(rename = "hasEarlier")]
    pub has_earlier: bool,
    #[serde(
        rename = "historyHasEarlier",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub history_has_earlier: Option<bool>,
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
///
/// [`SessionCreate`] 的载荷按间接层承载（[`Box`] 对 serde 透明：`Serialize` 与
/// `Deserialize` 的结果与非装箱时逐字节相同），否则它随 [`SessionSummary`] 增长会把整个枚举
/// 撑大到其余变体的数倍（`clippy::large_enum_variant`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum CommandResultPayload {
    /// `session.list` 的完成结果。
    SessionList(SessionListResult),
    /// `workspace.browse` 的完成结果。
    WorkspaceBrowse(WorkspaceBrowseResult),
    WorkspaceMkdir(WorkspaceMkdirResult),
    /// `session.create` 的完成结果。
    SessionCreate(Box<SessionCreateResult>),
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
                CommandName::SessionCreate => {
                    return parse_result::<SessionCreateResult>(
                        result,
                        "sessionCreateResult（session.create 的 completed）",
                    )
                    .map(|value| {
                        Nullable::value(CommandResultPayload::SessionCreate(Box::new(value)))
                    });
                }
                CommandName::WorkspaceMkdir => {
                    return parse_result::<WorkspaceMkdirResult>(result, "workspaceMkdirResult")
                        .map(|value| Nullable::value(CommandResultPayload::WorkspaceMkdir(value)));
                }
                CommandName::WorkspaceBrowse => {
                    return parse_result::<WorkspaceBrowseResult>(
                        result,
                        "workspaceBrowseResult（workspace.browse 的 completed）",
                    )
                    .map(|value| Nullable::value(CommandResultPayload::WorkspaceBrowse(value)));
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

#[cfg(test)]
mod tests {
    use super::*;

    const REQUEST: &str = "5f2a91c4-8d3b-4e77-b0a6-1c4d9e8f2a35";
    const SESSION: &str = "5d73cd10-a465-43cd-b3f1-704e2d49e99e";
    const TERMINAL: &str = "3a6b8d15-9c42-4f07-b1e8-5d7c2a9f0e31";
    const SUMMARY: &str = concat!(
        r#"{"sessionId":""#,
        "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
        r#"","title":null,"agent":{"agentId":"claude-code","name":"Claude Code"},"#,
        r#""state":"idle","origin":{"kind":"local"},"currentMode":null,"version":"1","#,
        r#""createdAt":"2026-09-17T12:12:01.000Z","updatedAt":"2026-09-17T12:12:01.000Z"}"#
    );

    fn body(command: &str, extra: &str, payload: &str) -> String {
        format!(r#"{{"requestId":"{REQUEST}","command":"{command}",{extra}"payload":{payload}}}"#)
    }

    #[test]
    fn command_name_covers_the_sync_subset_and_round_trips() {
        assert_eq!(CommandName::ALL.len(), 14, "sync 子集是 14 条命令");
        assert!(
            CommandName::ALL
                .iter()
                .all(|name| name.to_string() == name.as_str())
        );
        assert_eq!(
            CommandName::from_str("session.create"),
            Ok(CommandName::SessionCreate)
        );
        assert!(
            !CommandName::SessionCreate.is_query(),
            "session.create 是 mutation，不进查询命令集合"
        );
        assert!(
            !CommandName::SessionCreate.requires_session_id(),
            "会话尚不存在，body 顶层不得携带 sessionId"
        );
        assert!(!CommandName::SessionCreate.requires_expected_version());

        // workspace.browse 是查询命令，且不属于任何会话（与 session.list 同类禁止 sessionId）。
        assert_eq!(
            CommandName::from_str("workspace.browse"),
            Ok(CommandName::WorkspaceBrowse)
        );
        assert!(
            CommandName::WorkspaceBrowse.is_query(),
            "workspace.browse 是 query：completed 直接给结果且 terminalEventId 为 null"
        );
        assert!(
            !CommandName::WorkspaceBrowse.requires_session_id(),
            "浏览目录不属于任何会话，body 顶层不得携带 sessionId"
        );
        assert!(!CommandName::WorkspaceBrowse.requires_expected_version());
        assert_eq!(CommandName::WorkspaceBrowse.as_str(), "workspace.browse");
    }

    #[test]
    fn session_create_accepts_exactly_one_directory_source_plus_agent() {
        // 目录标识来源。
        let by_id: Command = serde_json::from_str(&body(
            "session.create",
            "",
            r#"{"directoryId":"0123456789abcdef0123456789abcdef","agentId":"claude-code"}"#,
        ))
        .expect("目录标识 + agentId 必须接受");
        assert_eq!(
            by_id.payload,
            CommandPayload::SessionCreate(SessionCreate {
                agent_id: NonEmptyText::parse("claude-code").expect("非空"),
                workspace: SessionCreateWorkspace::Directory(
                    NonEmptyText::parse("0123456789abcdef0123456789abcdef").expect("非空")
                ),
                create_if_missing: None,
            })
        );

        // 客户端提供的绝对路径来源。
        let by_path: Command = serde_json::from_str(&body(
            "session.create",
            "",
            r#"{"agentId":"claude-code","workspacePath":"/home/dev/projects/work-api"}"#,
        ))
        .expect("绝对路径 + agentId 必须接受");
        assert_eq!(
            by_path.payload,
            CommandPayload::SessionCreate(SessionCreate {
                agent_id: NonEmptyText::parse("claude-code").expect("非空"),
                workspace: SessionCreateWorkspace::Path(
                    NonEmptyText::parse("/home/dev/projects/work-api").expect("非空")
                ),
                create_if_missing: None,
            })
        );

        for rejected in [
            // oneOf：两个目录键都给或都不给都必须被拒。
            r#"{"agentId":"claude-code","directoryId":"0123456789abcdef0123456789abcdef","workspacePath":"/home/dev/projects/work-api"}"#,
            r#"{"agentId":"claude-code"}"#,
            // 目录键可缺失但不可为 null（它们不是可空字段）。
            r#"{"agentId":"claude-code","directoryId":null}"#,
            r#"{"agentId":"claude-code","workspacePath":null}"#,
            // 建目录开关与别名互斥：值是不是 true 都算「同时出现」（schema 的 not）。
            r#"{"directoryId":"0123456789abcdef0123456789abcdef","agentId":"claude-code","createIfMissing":true}"#,
            r#"{"directoryId":"0123456789abcdef0123456789abcdef","agentId":"claude-code","createIfMissing":false}"#,
            // 建目录开关是布尔键，没有 null 分支。
            r#"{"workspacePath":"/home/dev/projects/work-api","agentId":"claude-code","createIfMissing":null}"#,
            // cwd 与目录追加不是授权输入（SECURITY_DESIGN.md §12.3）。
            r#"{"directoryId":"0123456789abcdef0123456789abcdef","agentId":"claude-code","cwd":"C:\\repo"}"#,
            // 跨节点模板概念在 Sync 面不存在。
            r#"{"directoryId":"0123456789abcdef0123456789abcdef","agentId":"claude-code","templateParams":{}}"#,
            // 凭据与 MCP 配置同样不进入 wire。
            r#"{"directoryId":"0123456789abcdef0123456789abcdef","agentId":"claude-code","mcpServers":{}}"#,
            // 空串不满足取值域；agentId 必填。
            r#"{"directoryId":"","agentId":"claude-code"}"#,
            r#"{"workspacePath":"","agentId":"claude-code"}"#,
            r#"{"directoryId":"0123456789abcdef0123456789abcdef"}"#,
        ] {
            assert!(
                serde_json::from_str::<Command>(&body("session.create", "", rejected)).is_err(),
                "payload 必须被拒：{rejected}"
            );
        }

        // 手写 `Serialize` 必须回到 wire 形状（`payload` 是 `untagged` 枚举的一支）。
        assert_eq!(
            serde_json::to_string(&by_path.payload).expect("必须可序列化"),
            r#"{"agentId":"claude-code","workspacePath":"/home/dev/projects/work-api"}"#
        );

        // 建目录开关：与客户端提供的目录可以同时出现，且键缺席与键在场（false）必须可区分。
        let with_create: Command = serde_json::from_str(&body(
            "session.create",
            "",
            r#"{"agentId":"claude-code","workspacePath":"/home/dev/projects/work-api","createIfMissing":true}"#,
        ))
        .expect("绝对路径 + 建目录开关必须接受");
        let CommandPayload::SessionCreate(payload) = with_create.payload else {
            panic!("payload 必须是 SessionCreate");
        };
        assert_eq!(
            payload.create_if_missing,
            Some(true),
            "开关必须逐字承载服务端要执行的语义"
        );
        assert_eq!(
            serde_json::to_string(&CommandPayload::SessionCreate(payload)).expect("必须可序列化"),
            r#"{"agentId":"claude-code","workspacePath":"/home/dev/projects/work-api","createIfMissing":true}"#,
            "开关在场时必须写回 wire，不得被 skip_serializing_if 吞掉"
        );

        let absent: Command = serde_json::from_str(&body(
            "session.create",
            "",
            r#"{"agentId":"claude-code","workspacePath":"/home/dev/projects/work-api"}"#,
        ))
        .expect("省略开关必须接受");
        let CommandPayload::SessionCreate(payload) = absent.payload else {
            panic!("payload 必须是 SessionCreate");
        };
        assert_eq!(
            payload.create_if_missing, None,
            "键缺席即「不创建」：不得被读成 false 之外的任何默认"
        );

        let explicit_false: Command = serde_json::from_str(&body(
            "session.create",
            "",
            r#"{"agentId":"claude-code","workspacePath":"/home/dev/projects/work-api","createIfMissing":false}"#,
        ))
        .expect("显式 false 必须接受");
        let CommandPayload::SessionCreate(payload) = explicit_false.payload else {
            panic!("payload 必须是 SessionCreate");
        };
        assert_eq!(
            payload.create_if_missing,
            Some(false),
            "显式 false 与键缺席在 wire 上是两种形状（对端必须能逐字区分）"
        );

        // 顶层 sessionId 由 `additionalProperties: false` 拒绝（不是被静默忽略）。
        let with_session = format!(
            r#"{{"requestId":"{REQUEST}","command":"session.create","sessionId":"{SESSION}","payload":{{"directoryId":"0123456789abcdef0123456789abcdef","agentId":"claude-code"}}}}"#
        );
        assert!(serde_json::from_str::<Command>(&with_session).is_err());
    }

    #[test]
    fn session_create_result_is_bound_to_completed() {
        let completed = format!(
            r#"{{"requestId":"{REQUEST}","command":"session.create","status":"completed","acceptedAt":"2026-09-17T12:12:00.000Z","terminalEventId":"{TERMINAL}","result":{{"sessionId":"{SESSION}","session":{SUMMARY}}},"error":null}}"#
        );
        let result: CommandResult = serde_json::from_str(&completed).expect("completed 必须接受");
        let payload = match result.result.as_ref() {
            Some(CommandResultPayload::SessionCreate(payload)) => payload,
            other => panic!("completed 的 result 必须是 sessionCreateResult：{other:?}"),
        };
        assert_eq!(
            payload.session_id.as_str(),
            SESSION,
            "sessionId 必须是载荷里的那个"
        );
        // 装箱只是内存表示：重编码必须与 wire 原文逐字节相同。
        assert_eq!(
            serde_json::to_string(result.result.as_ref().expect("completed 必须有 result"))
                .expect("CommandResultPayload 必须可序列化"),
            format!(r#"{{"sessionId":"{SESSION}","session":{SUMMARY}}}"#),
            "Box 对 serde 透明：装箱不得改变 wire"
        );

        // 缺少 `session` 的结果不得按基类型 `object` 放行。
        let missing = completed.replace(&format!(r#","session":{SUMMARY}"#), "");
        assert!(serde_json::from_str::<CommandResult>(&missing).is_err());

        // mutation 的 completed 必须给出唯一 terminalEventId。
        let no_terminal = completed.replace(
            &format!(r#""terminalEventId":"{TERMINAL}""#),
            r#""terminalEventId":null"#,
        );
        assert!(serde_json::from_str::<CommandResult>(&no_terminal).is_err());

        // accepted：result 与 terminalEventId 均为 null，error 为 null。
        let accepted = format!(
            r#"{{"requestId":"{REQUEST}","command":"session.create","status":"accepted","acceptedAt":"2026-09-17T12:12:00.000Z","terminalEventId":null,"result":null,"error":null}}"#
        );
        let accepted: CommandResult = serde_json::from_str(&accepted).expect("accepted 必须接受");
        assert!(accepted.result.is_null());
    }

    #[test]
    fn workspace_browse_takes_one_nullable_path_and_binds_its_result() {
        // 根列表：`path` 为 `null`。它是**键在场且值为 null**，不得被写成键缺席。
        let root: Command = serde_json::from_str(&body("workspace.browse", "", r#"{"path":null}"#))
            .expect("path 为 null（根列表）必须接受");
        let CommandPayload::WorkspaceBrowse(payload) = root.payload else {
            panic!("payload 必须是 WorkspaceBrowse");
        };
        assert!(payload.path.is_null(), "null 表示请求该节点的根列表");
        assert_eq!(
            serde_json::to_string(&CommandPayload::WorkspaceBrowse(payload)).expect("可序列化"),
            r#"{"path":null}"#,
            "根列表必须逐字写成 path: null，不得被压成键缺席"
        );

        // 某一层：绝对路径逐字保留（规范化与可达性判定属于本机用例）。
        let layer: Command =
            serde_json::from_str(&body("workspace.browse", "", r#"{"path":"D:\\work\\api"}"#))
                .expect("绝对路径必须接受");
        let CommandPayload::WorkspaceBrowse(payload) = layer.payload else {
            panic!("payload 必须是 WorkspaceBrowse");
        };
        assert_eq!(
            payload.path.as_ref().map(|path| path.as_str()),
            Some(r"D:\work\api")
        );

        // 这一层只判定 shape：相对路径与含 `..` 由本机用例分类（不在这里拒绝），但空串、缺键、
        // 多余键与非字符串必须被拒。
        for rejected in [
            r#"{}"#,
            r#"{"path":""}"#,
            r#"{"path":"D:\\work","entries":[]}"#,
            r#"{"path":"D:\\work","recursive":true}"#,
            r#"{"path":7}"#,
        ] {
            assert!(
                serde_json::from_str::<Command>(&body("workspace.browse", "", rejected)).is_err(),
                "payload 必须被拒：{rejected}"
            );
        }

        // 浏览不属于任何会话：顶层 sessionId 由 `additionalProperties: false` 拒绝。
        let with_session = format!(
            r#"{{"requestId":"{REQUEST}","command":"workspace.browse","sessionId":"{SESSION}","payload":{{"path":null}}}}"#
        );
        assert!(serde_json::from_str::<Command>(&with_session).is_err());

        // completed 的结果必须落在自己的形状上（workspaceBrowseResult），且逐字承载截断标记与登记状态。
        let completed = format!(
            r#"{{"requestId":"{REQUEST}","command":"workspace.browse","status":"completed","acceptedAt":"2026-09-17T12:12:00.000Z","terminalEventId":null,"result":{{"path":null,"entries":[{{"name":"work","path":"D:\\work","registered":true}},{{"name":"tmp","path":"D:\\tmp","registered":false}}],"truncated":true}},"error":null}}"#
        );
        let parsed: CommandResult = serde_json::from_str(&completed).expect("completed 必须接受");
        let payload = match parsed.result.as_ref() {
            Some(CommandResultPayload::WorkspaceBrowse(payload)) => payload,
            other => panic!("completed 的 result 必须是 workspaceBrowseResult：{other:?}"),
        };
        assert!(payload.truncated, "截断标记必须逐字承载");
        assert!(payload.entries[0].registered, "已登记的条目必须标出");
        assert!(!payload.entries[1].registered);

        for rejected in [
            // 截断标记是必填的：缺席不得按默认值放行。
            r#"{"path":null,"entries":[]}"#,
            // 条目的三个字段都是必填。
            r#"{"path":null,"entries":[{"name":"work","path":"D:\\work"}],"truncated":false}"#,
            // 文件内容与元数据不在 wire 上：多一个键即拒。
            r#"{"path":null,"entries":[{"name":"work","path":"D:\\work","registered":false,"size":"12"}],"truncated":false}"#,
            // 回显的父路径是必填字段。
            r#"{"entries":[],"truncated":false}"#,
        ] {
            let wire = format!(
                r#"{{"requestId":"{REQUEST}","command":"workspace.browse","status":"completed","acceptedAt":"2026-09-17T12:12:00.000Z","terminalEventId":null,"result":{rejected},"error":null}}"#
            );
            assert!(
                serde_json::from_str::<CommandResult>(&wire).is_err(),
                "result 必须被拒：{rejected}"
            );
        }

        // 查询命令的 completed 必须 terminalEventId = null（mutation 才带终态事件）。
        let with_terminal = completed.replace(
            r#""terminalEventId":null"#,
            &format!(r#""terminalEventId":"{TERMINAL}""#),
        );
        assert!(serde_json::from_str::<CommandResult>(&with_terminal).is_err());

        // 别的命令的完成结果形状不得被放行。
        let mismatched = completed.replace(
            r#""command":"workspace.browse""#,
            r#""command":"session.list""#,
        );
        assert!(serde_json::from_str::<CommandResult>(&mismatched).is_err());
    }

    #[test]
    fn session_read_paging_is_optional_and_the_cursor_is_composite() {
        let include_only = body(
            "session.read",
            &format!(r#""sessionId":"{SESSION}","#),
            r#"{"include":["messages"]}"#,
        );
        let command: Command =
            serde_json::from_str(&include_only).expect("只给 include 必须接受：默认页由服务端决定");
        let payload = match command.payload {
            CommandPayload::SessionRead(payload) => payload,
            other => panic!("payload 必须是 SessionRead：{other:?}"),
        };
        assert_eq!(payload.before, None, "省略 before 即请求最新一页");
        assert_eq!(payload.limit, None, "省略 limit 不得被要求显式给出页大小");

        let paged = body(
            "session.read",
            &format!(r#""sessionId":"{SESSION}","#),
            r#"{"include":["messages"],"before":{"createdAt":"2026-09-17T12:00:00.000Z","messageId":"7c1d4e6f-8a90-4b2c-9d3e-5f6a7b8c9d0e"},"limit":20}"#,
        );
        let payload = match serde_json::from_str::<Command>(&paged)
            .expect("复合游标必须接受")
            .payload
        {
            CommandPayload::SessionRead(payload) => payload,
            other => panic!("payload 必须是 SessionRead：{other:?}"),
        };
        let before = payload.before.expect("before 必须被解析成复合游标");
        assert_eq!(
            before.message_id.as_str(),
            "7c1d4e6f-8a90-4b2c-9d3e-5f6a7b8c9d0e"
        );
        assert_eq!(payload.limit.expect("limit 必须被解析").get(), 20);

        for rejected in [
            // 单字段游标：只给 messageId 不得被补全 createdAt（design D2 / §11.5）。
            r#"{"include":["messages"],"before":{"messageId":"7c1d4e6f-8a90-4b2c-9d3e-5f6a7b8c9d0e"}}"#,
            // 单字段游标：只给 createdAt 同理。
            r#"{"include":["messages"],"before":{"createdAt":"2026-09-17T12:00:00.000Z"}}"#,
            // 游标只接受这两个键：拿 messageId 当排序依据的自造字段不是形状的一部分。
            r#"{"include":["messages"],"before":{"createdAt":"2026-09-17T12:00:00.000Z","messageId":"7c1d4e6f-8a90-4b2c-9d3e-5f6a7b8c9d0e","offset":20}}"#,
            // limit 是结构性计数：0 与小数都不是合法页大小。
            r#"{"include":["messages"],"limit":0}"#,
            r#"{"include":["messages"],"limit":20.5}"#,
            // 偏移量分页是被否决的替代（design D2）：不得出现在 wire 上。
            r#"{"include":["messages"],"offset":20}"#,
        ] {
            assert!(
                serde_json::from_str::<Command>(&body(
                    "session.read",
                    &format!(r#""sessionId":"{SESSION}","#),
                    rejected,
                ))
                .is_err(),
                "payload 必须被拒：{rejected}"
            );
        }
    }

    #[test]
    fn session_read_result_carries_has_earlier() {
        let result = |has_earlier: &str| {
            format!(
                r#"{{"requestId":"{REQUEST}","command":"session.read","status":"completed","acceptedAt":"2026-09-17T12:12:00.000Z","terminalEventId":null,"result":{{"sessionId":"{SESSION}","resources":{{"messages":[]}},"hasEarlier":{has_earlier}}},"error":null}}"#
            )
        };

        for (wire, expected) in [("true", true), ("false", false)] {
            let parsed: CommandResult =
                serde_json::from_str(&result(wire)).expect("hasEarlier 两态都必须接受");
            let payload = match parsed.result.as_ref() {
                Some(CommandResultPayload::SessionRead(payload)) => payload,
                other => panic!("completed 的 result 必须是 sessionReadResult：{other:?}"),
            };
            assert_eq!(
                payload.has_earlier, expected,
                "hasEarlier 必须逐字承载服务端结论"
            );
        }

        // 必填：缺席时不得按默认值放行——客户端无法区分「没有更早内容」与「服务端没说」。
        let missing = result("true").replace(r#","hasEarlier":true"#, "");
        assert!(serde_json::from_str::<CommandResult>(&missing).is_err());
    }

    #[test]
    fn session_read_history_include_and_cursor_follow_the_shared_vocabulary() {
        assert_eq!(IncludeResource::ALL.len(), 6, "include 的取值集合");
        assert_eq!(
            IncludeResource::History.as_str(),
            "history",
            "共享词表：include 新增的取值逐字是 history"
        );
        assert_eq!(
            IncludeResource::from_str("history"),
            Ok(IncludeResource::History)
        );

        let payload = |payload: &str| {
            serde_json::from_str::<Command>(&body(
                "session.read",
                &format!(r#""sessionId":"{SESSION}","#),
                payload,
            ))
        };

        let only_history =
            payload(r#"{"include":["history"]}"#).expect("只请求 history 必须被接受");
        let CommandPayload::SessionRead(read) = only_history.payload else {
            panic!("payload 必须是 SessionRead");
        };
        assert_eq!(read.include.as_slice().to_vec(), [IncludeResource::History]);
        assert_eq!(
            read.before_history, None,
            "省略 beforeHistory 即请求最新一页"
        );
        assert_eq!(read.before, None, "省略 before 仍是请求最新消息页");

        let both = payload(
            r#"{"include":["messages","history"],"before":{"createdAt":"2026-09-17T12:00:00.000Z","messageId":"7c1d4e6f-8a90-4b2c-9d3e-5f6a7b8c9d0e"},"beforeHistory":"105"}"#,
        )
        .expect("两套游标必须能同时出现在同一请求里");
        let CommandPayload::SessionRead(read) = both.payload else {
            panic!("payload 必须是 SessionRead");
        };
        assert_eq!(
            read.before_history.as_ref().map(|cursor| cursor.as_str()),
            Some("105"),
            "beforeHistory 是十进制字符串形式的会话内事件序号"
        );
        assert!(
            read.before.is_some(),
            "历史游标不得挤掉消息游标：两者各翻各自的页"
        );

        for rejected in [
            // 会话内事件序号是十进制字符串：事件标识不是它的替身（design D2）。
            r#"{"include":["history"],"beforeHistory":"3c7e5a91-6b2d-4c88-bd0f-8a1e2f3c4d56"}"#,
            // 结构性计数走十进制字符串规则（§3.3）：JSON 数字与 null 都不是合法游标。
            r#"{"include":["history"],"beforeHistory":105}"#,
            r#"{"include":["history"],"beforeHistory":null}"#,
            // 未登记的 include 取值不得被静默忽略；去重规则同样适用。
            r#"{"include":["histories"]}"#,
            r#"{"include":["history","history"]}"#,
        ] {
            assert!(payload(rejected).is_err(), "payload 必须被拒：{rejected}");
        }
    }

    #[test]
    fn session_read_result_carries_history_entries_and_a_separate_page_marker() {
        // `historyHasEarlier` 省略（None）与取 `null` 是两件事：前者表示本次没请求 history，
        // 后者是形状错误（字段没有 null 分支）。
        let result = |history_has_earlier: Option<&str>| {
            let marker = match history_has_earlier {
                Some(value) => format!(r#","historyHasEarlier":{value}"#),
                None => String::new(),
            };
            format!(
                r#"{{"requestId":"{REQUEST}","command":"session.read","status":"completed","acceptedAt":"2026-09-17T12:12:00.000Z","terminalEventId":null,"result":{{"sessionId":"{SESSION}","resources":{{"messages":[],"history":[{{"eventId":"9c1d7a45-2f68-4b03-8e19-4a7c5d2b6f80","eventType":"tool.call.completed","sessionSequence":"103","createdAt":"2026-09-17T11:57:40.000Z","view":{{}}}}]}},"hasEarlier":false{marker}}},"error":null}}"#
            )
        };

        let history_of = |wire: &str| {
            let parsed: CommandResult =
                serde_json::from_str(wire).expect("带 history 的完成结果必须接受");
            match parsed.result.as_ref() {
                Some(CommandResultPayload::SessionRead(payload)) => (
                    payload.resources.history.clone(),
                    payload.history_has_earlier,
                    payload.has_earlier,
                ),
                other => panic!("completed 的 result 必须是 sessionReadResult：{other:?}"),
            }
        };

        for (wire, expected) in [("true", true), ("false", false)] {
            let (history, page_marker, has_earlier) = history_of(&result(Some(wire)));
            let history = history.expect("history 必须被解析成条目数组");
            assert_eq!(history.len(), 1, "history 的元素数");
            assert_eq!(history[0].session_sequence.as_str(), "103", "会话内序号");
            assert_eq!(history[0].event_type.as_str(), "tool.call.completed");
            assert_eq!(
                history[0].acp, None,
                "acp 可缺席（原文不可用时也不得改形状）"
            );
            assert_eq!(
                page_marker,
                Some(expected),
                "historyHasEarlier 两态都必须逐字承载"
            );
            assert!(!has_earlier, "hasEarlier 仍只描述消息分页");
        }

        let (_, page_marker, _) = history_of(&result(None));
        assert_eq!(page_marker, None, "缺席 historyHasEarlier 即未请求 history");
        assert!(
            serde_json::from_str::<CommandResult>(&result(Some("null"))).is_err(),
            "historyHasEarlier 没有 null 分支：显式 null 必须被拒"
        );
    }
}
