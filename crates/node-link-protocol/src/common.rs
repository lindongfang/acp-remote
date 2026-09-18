//! Node Link v1 的协议专属公共类型（`schemas/node-link/v1/common.schema.json`）。
//!
//! 跨协议共用的值对象与字段校验机制（`uuid`、`decimalString`、`timestamp`、`base64url*`、`featureId`、
//! `featureList`、`rawAcp`、`Nullable`、`RawObject`、`ExtraFields`、`Text`、`NonEmptyText`、`BoundedU64`、
//! `ValueError`）来自叶子 crate `acpr-wire`（[ADR-0007](../../../docs/adr/0007-shared-wire-value-crate.md)），
//! 这里只做命名空间再导出；本文件拥有的是 Node Link 自己的值对象——节点、Export、workspace 模板、origin
//! cursor、会话元数据与 limits。
//!
//! 校验只发生在反序列化：构造出来的值必然满足对应 schema，后续层不需要重复校验。

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use acpr_wire::{
    Base64Url, BoundedU64, DecimalString, ExtraFields, FeatureId, FeatureList, NonEmptyText,
    Nullable, ProtocolVersionV1, RawAcp, RawObject, Text, Timestamp, UIntAtLeast, Uuid, ValueError,
    deserialize_optional_non_null,
};

/// `common.schema.json#/$defs/publicError`，以 Node Link 自己的错误码枚举具体化。
///
/// 类型定义在 `crate::error`（错误码词表在那里），这里只做命名空间再导出，让家族模块统一从
/// `crate::common` 取值。
pub use crate::error::PublicError;

/// `common.schema.json#/$defs/nodeName`：1..=128 的文本，无 pattern。
pub type NodeName = NonEmptyText<128>;

/// 按 schema 的 `pattern`（与可选长度上限）校验的字符串 newtype。
///
/// 这些类型共享同一套「取值域只在反序列化校验」的形状，用宏生成可以保证四个 impl 不会各自写歪；
/// 每个类型的文档注释仍指向它对应的 `$defs`。
macro_rules! pattern_string {
    ($(#[$meta:meta])* $name:ident, $pattern:expr, $max:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// 按 schema 的 `pattern` 校验；`$max` 是字符数上限。
            pub fn parse(text: &str) -> Result<Self, ValueError> {
                let valid =
                    !text.is_empty() && text.chars().count() <= $max && $pattern(text);
                if !valid {
                    return Err(ValueError::Pattern {
                        field: stringify!($name),
                        value: text.to_owned(),
                    });
                }
                Ok(Self(text.to_owned()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                Self::parse(&text).map_err(DeError::custom)
            }
        }
    };
}

/// 只含 ASCII 字母、数字、`_`、`.`、`-` 的文本（schema 的 `^[A-Za-z0-9._-]+$`）。
fn is_ascii_ref(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

/// `^[a-z0-9][a-z0-9._-]{0,63}$`：workspace 符号名，不是路径。
fn is_workspace_alias(text: &str) -> bool {
    let mut bytes = text.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() => {}
        _ => return false,
    }
    text.len() <= 64
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

/// `^[A-Za-z_][A-Za-z0-9_]{0,63}$`：模板参数名。
fn is_template_param_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    match bytes.next() {
        Some(first) if first.is_ascii_alphabetic() || first == b'_' => {}
        _ => return false,
    }
    text.len() <= 64 && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pattern_string!(
    /// `common.schema.json#/$defs/exportId`：`^[A-Za-z0-9._-]+$`，≤128。
    ExportId,
    is_ascii_ref,
    128
);
pattern_string!(
    /// `common.schema.json#/$defs/agentId`：1..=128 的文本（schema 只约束长度）。
    AgentId,
    |text: &str| !text.is_empty(),
    128
);
pattern_string!(
    /// `common.schema.json#/$defs/opaqueRef`：Owner 侧不透明引用，接收方只能原样回传。
    OpaqueRef,
    is_ascii_ref,
    128
);
pattern_string!(
    /// `common.schema.json#/$defs/templateId`：`^[A-Za-z0-9._-]+$`，≤128。
    TemplateId,
    is_ascii_ref,
    128
);
pattern_string!(
    /// `common.schema.json#/$defs/workspaceAlias`：`^[a-z0-9][a-z0-9._-]{0,63}$`，不是路径。
    WorkspaceAlias,
    is_workspace_alias,
    64
);
pattern_string!(
    /// `workspaceTemplateParam.name`：`^[A-Za-z_][A-Za-z0-9_]{0,63}$`。
    TemplateParamName,
    is_template_param_name,
    64
);

/// `common.schema.json#/$defs/nodeKind`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Owner,
    Access,
}

impl NodeKind {
    pub const ALL: [NodeKind; 2] = [NodeKind::Owner, NodeKind::Access];

    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Owner => "owner",
            NodeKind::Access => "access",
        }
    }
}

impl fmt::Display for NodeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for NodeKind {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        NodeKind::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "nodeKind",
                value: text.to_owned(),
            })
    }
}

impl Serialize for NodeKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for NodeKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        NodeKind::from_str(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/grantName`。
///
/// 词表的唯一来源是 `compatibility/commands/v1/commands.json` 的 `grants` 键，由契约测试逐条断言；
/// 这里不自行增删取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrantName {
    Observe,
    Interact,
    ConfigureSession,
    Approve,
    RemoteWork,
}

impl GrantName {
    pub const ALL: [GrantName; 5] = [
        GrantName::Observe,
        GrantName::Interact,
        GrantName::ConfigureSession,
        GrantName::Approve,
        GrantName::RemoteWork,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            GrantName::Observe => "grant.observe",
            GrantName::Interact => "grant.interact",
            GrantName::ConfigureSession => "grant.configure-session",
            GrantName::Approve => "grant.approve",
            GrantName::RemoteWork => "grant.remote-work",
        }
    }
}

impl fmt::Display for GrantName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for GrantName {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        GrantName::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "grantName",
                value: text.to_owned(),
            })
    }
}

impl Serialize for GrantName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for GrantName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        GrantName::from_str(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/grantList`：去重的 grant 列表（`uniqueItems`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrantList(Vec<GrantName>);

impl GrantList {
    pub fn new(grants: Vec<GrantName>) -> Result<Self, ValueError> {
        let list = Self(grants);
        list.validate()?;
        Ok(list)
    }

    pub fn as_slice(&self) -> &[GrantName] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn validate(&self) -> Result<(), ValueError> {
        let mut seen = BTreeSet::new();
        for grant in &self.0 {
            if !seen.insert(grant.as_str()) {
                return Err(ValueError::RepeatedItem {
                    field: "scopes",
                    value: grant.as_str().to_owned(),
                });
            }
        }
        Ok(())
    }
}

impl Serialize for GrantList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GrantList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let list = Self(Vec::<GrantName>::deserialize(deserializer)?);
        list.validate().map_err(DeError::custom)?;
        Ok(list)
    }
}

/// `common.schema.json#/$defs/originCursor`：跨节点排序一律以 `(originEpoch, originSequence)` 为准。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginCursor {
    #[serde(rename = "originEpoch")]
    pub origin_epoch: Uuid,
    #[serde(rename = "originSequence")]
    pub origin_sequence: DecimalString,
}

/// `common.schema.json#/$defs/remoteSessionRef`：Owner 侧会话的跨节点引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteSessionRef {
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "exportId")]
    pub export_id: ExportId,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
}

/// `common.schema.json#/$defs/sessionState`：与 Sync 的 `sessionSummary.state` 同一取值集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionState {
    Idle,
    Queued,
    Running,
    WaitingInput,
    WaitingPermission,
    Failed,
    Closed,
}

impl SessionState {
    pub const ALL: [SessionState; 7] = [
        SessionState::Idle,
        SessionState::Queued,
        SessionState::Running,
        SessionState::WaitingInput,
        SessionState::WaitingPermission,
        SessionState::Failed,
        SessionState::Closed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::Idle => "idle",
            SessionState::Queued => "queued",
            SessionState::Running => "running",
            SessionState::WaitingInput => "waiting_input",
            SessionState::WaitingPermission => "waiting_permission",
            SessionState::Failed => "failed",
            SessionState::Closed => "closed",
        }
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for SessionState {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        SessionState::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "state",
                value: text.to_owned(),
            })
    }
}

impl Serialize for SessionState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SessionState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        SessionState::from_str(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/sessionMeta`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionMeta {
    pub state: SessionState,
    pub version: DecimalString,
}

/// `common.schema.json#/$defs/sessionCreateResult`：`session.create` 的成功终态结果。
///
/// `command.accepted` 的 `result` 必须为 `null`；会话创建完成后由 `command.terminal` 携带本形状
/// （`docs/NODE_LINK_PROTOCOL.md` §12.7）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCreateResult {
    #[serde(rename = "remoteSessionRef")]
    pub remote_session_ref: RemoteSessionRef,
    #[serde(rename = "sessionMeta")]
    pub session_meta: SessionMeta,
}

/// `common.schema.json#/$defs/pendingInteraction.kind`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionKind {
    Permission,
    Elicitation,
}

impl InteractionKind {
    pub const ALL: [InteractionKind; 2] =
        [InteractionKind::Permission, InteractionKind::Elicitation];

    pub fn as_str(self) -> &'static str {
        match self {
            InteractionKind::Permission => "permission",
            InteractionKind::Elicitation => "elicitation",
        }
    }
}

impl fmt::Display for InteractionKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for InteractionKind {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        InteractionKind::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "kind",
                value: text.to_owned(),
            })
    }
}

impl Serialize for InteractionKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for InteractionKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        InteractionKind::from_str(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/pendingInteraction`：待处理交互的索引条目（无正文，只有摘要）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingInteraction {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    pub kind: InteractionKind,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    #[serde(rename = "payloadDigest")]
    pub payload_digest: Base64Url<32>,
}

/// `common.schema.json#/$defs/exportAgent`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportAgent {
    #[serde(rename = "agentId")]
    pub agent_id: AgentId,
    pub name: NonEmptyText<128>,
    #[serde(rename = "capabilitiesRef")]
    pub capabilities_ref: OpaqueRef,
}

/// `common.schema.json#/$defs/workspaceAliasEntry`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceAliasEntry {
    pub alias: WorkspaceAlias,
    #[serde(rename = "displayName")]
    pub display_name: NonEmptyText<128>,
}

/// `workspaceTemplateParam.type`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateParamType {
    String,
    Boolean,
    Integer,
}

impl TemplateParamType {
    pub const ALL: [TemplateParamType; 3] = [
        TemplateParamType::String,
        TemplateParamType::Boolean,
        TemplateParamType::Integer,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TemplateParamType::String => "string",
            TemplateParamType::Boolean => "boolean",
            TemplateParamType::Integer => "integer",
        }
    }
}

impl fmt::Display for TemplateParamType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for TemplateParamType {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        TemplateParamType::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "type",
                value: text.to_owned(),
            })
    }
}

impl Serialize for TemplateParamType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TemplateParamType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        TemplateParamType::from_str(&text).map_err(DeError::custom)
    }
}

/// `workspaceTemplateParam.enum`：1..=256 项的字符串集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateParamValues(Vec<String>);

impl TemplateParamValues {
    pub fn new(values: Vec<String>) -> Result<Self, ValueError> {
        if values.is_empty() || values.len() > 256 {
            return Err(ValueError::TooManyItems {
                field: "enum",
                max: 256,
                actual: values.len(),
            });
        }
        Ok(Self(values))
    }

    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

impl Serialize for TemplateParamValues {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TemplateParamValues {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<String>::deserialize(deserializer)?;
        Self::new(values).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/workspaceTemplateParam`。
///
/// `pattern` 与 `enum` 都是 required 且可为 `null`（分别约束 `type` 为 `string` 时的取值），因此用
/// [`Nullable`] 而不是 `Option`：键必须出现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceTemplateParam {
    pub name: TemplateParamName,
    #[serde(rename = "type")]
    pub param_type: TemplateParamType,
    pub required: bool,
    pub pattern: Nullable<NonEmptyText<512>>,
    #[serde(rename = "enum")]
    pub values: Nullable<TemplateParamValues>,
}

/// `common.schema.json#/$defs/workspaceTemplate`（`params` 最多 32 项）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTemplate {
    pub template_id: TemplateId,
    pub display_name: NonEmptyText<128>,
    pub workspace_alias: WorkspaceAlias,
    pub params: Vec<WorkspaceTemplateParam>,
}

const MAX_TEMPLATE_PARAMS: usize = 32;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceTemplate {
    #[serde(rename = "templateId")]
    template_id: TemplateId,
    #[serde(rename = "displayName")]
    display_name: NonEmptyText<128>,
    #[serde(rename = "workspaceAlias")]
    workspace_alias: WorkspaceAlias,
    params: Vec<WorkspaceTemplateParam>,
}

impl TryFrom<RawWorkspaceTemplate> for WorkspaceTemplate {
    type Error = ValueError;

    fn try_from(raw: RawWorkspaceTemplate) -> Result<Self, Self::Error> {
        if raw.params.len() > MAX_TEMPLATE_PARAMS {
            return Err(ValueError::TooManyItems {
                field: "params",
                max: MAX_TEMPLATE_PARAMS,
                actual: raw.params.len(),
            });
        }
        Ok(Self {
            template_id: raw.template_id,
            display_name: raw.display_name,
            workspace_alias: raw.workspace_alias,
            params: raw.params,
        })
    }
}

impl<'de> Deserialize<'de> for WorkspaceTemplate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawWorkspaceTemplate::deserialize(deserializer)?;
        Self::try_from(raw).map_err(DeError::custom)
    }
}

impl Serialize for WorkspaceTemplate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Out<'a> {
            template_id: &'a TemplateId,
            display_name: &'a NonEmptyText<128>,
            workspace_alias: &'a WorkspaceAlias,
            params: &'a Vec<WorkspaceTemplateParam>,
        }
        // `TemplateId`/`WorkspaceAlias` 等 newtype 的 `Serialize` 已是字符串，字段名由 `Out` 负责。
        Out {
            template_id: &self.template_id,
            display_name: &self.display_name,
            workspace_alias: &self.workspace_alias,
            params: &self.params,
        }
        .serialize(serializer)
    }
}

/// `exportEntry.cachePolicy`：v1 固定 `no-content-cache`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoContentCache;

impl NoContentCache {
    pub const VALUE: &'static str = "no-content-cache";
}

impl Serialize for NoContentCache {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(Self::VALUE)
    }
}

impl<'de> Deserialize<'de> for NoContentCache {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text != Self::VALUE {
            return Err(DeError::custom(ValueError::Enumerated {
                field: "cachePolicy",
                value: text,
            }));
        }
        Ok(Self)
    }
}

/// `common.schema.json#/$defs/exportEntry`：Owner 对一个 Access Node 可见的导出条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportEntry {
    pub export_id: ExportId,
    pub display_name: NonEmptyText<128>,
    pub agents: Vec<ExportAgent>,
    pub workspace_aliases: Vec<WorkspaceAliasEntry>,
    pub default_workspace_alias: WorkspaceAlias,
    pub templates: Vec<WorkspaceTemplate>,
    pub scopes: GrantList,
    pub capability_ceiling_ref: OpaqueRef,
    pub cache_policy: NoContentCache,
    pub revoked: bool,
}

const MAX_EXPORT_AGENTS: usize = 1024;
const MAX_EXPORT_ALIASES: usize = 1024;
const MAX_EXPORT_TEMPLATES: usize = 64;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExportEntry {
    #[serde(rename = "exportId")]
    export_id: ExportId,
    #[serde(rename = "displayName")]
    display_name: NonEmptyText<128>,
    agents: Vec<ExportAgent>,
    #[serde(rename = "workspaceAliases")]
    workspace_aliases: Vec<WorkspaceAliasEntry>,
    #[serde(rename = "defaultWorkspaceAlias")]
    default_workspace_alias: WorkspaceAlias,
    templates: Vec<WorkspaceTemplate>,
    scopes: GrantList,
    #[serde(rename = "capabilityCeilingRef")]
    capability_ceiling_ref: OpaqueRef,
    #[serde(rename = "cachePolicy")]
    cache_policy: NoContentCache,
    revoked: bool,
}

impl TryFrom<RawExportEntry> for ExportEntry {
    type Error = ValueError;

    fn try_from(raw: RawExportEntry) -> Result<Self, Self::Error> {
        for (field, actual, max) in [
            ("agents", raw.agents.len(), MAX_EXPORT_AGENTS),
            (
                "workspaceAliases",
                raw.workspace_aliases.len(),
                MAX_EXPORT_ALIASES,
            ),
            ("templates", raw.templates.len(), MAX_EXPORT_TEMPLATES),
        ] {
            if actual > max {
                return Err(ValueError::TooManyItems { field, max, actual });
            }
        }
        Ok(Self {
            export_id: raw.export_id,
            display_name: raw.display_name,
            agents: raw.agents,
            workspace_aliases: raw.workspace_aliases,
            default_workspace_alias: raw.default_workspace_alias,
            templates: raw.templates,
            scopes: raw.scopes,
            capability_ceiling_ref: raw.capability_ceiling_ref,
            cache_policy: raw.cache_policy,
            revoked: raw.revoked,
        })
    }
}

impl<'de> Deserialize<'de> for ExportEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawExportEntry::deserialize(deserializer)?;
        Self::try_from(raw).map_err(DeError::custom)
    }
}

impl Serialize for ExportEntry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Out<'a> {
            export_id: &'a ExportId,
            display_name: &'a NonEmptyText<128>,
            agents: &'a Vec<ExportAgent>,
            workspace_aliases: &'a Vec<WorkspaceAliasEntry>,
            default_workspace_alias: &'a WorkspaceAlias,
            templates: &'a Vec<WorkspaceTemplate>,
            scopes: &'a GrantList,
            capability_ceiling_ref: &'a OpaqueRef,
            cache_policy: &'a NoContentCache,
            revoked: bool,
        }
        Out {
            export_id: &self.export_id,
            display_name: &self.display_name,
            agents: &self.agents,
            workspace_aliases: &self.workspace_aliases,
            default_workspace_alias: &self.default_workspace_alias,
            templates: &self.templates,
            scopes: &self.scopes,
            capability_ceiling_ref: &self.capability_ceiling_ref,
            cache_policy: &self.cache_policy,
            revoked: self.revoked,
        }
        .serialize(serializer)
    }
}

/// `common.schema.json#/$defs/limits`：`node.ready` 下发的 Node Link 限额。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeLinkLimits {
    #[serde(rename = "maxMessageBytes")]
    pub max_message_bytes: UIntAtLeast<1024>,
    #[serde(rename = "catalogSnapshotBatchSize")]
    pub catalog_snapshot_batch_size: UIntAtLeast<1>,
    #[serde(rename = "resourceSnapshotBatchSize")]
    pub resource_snapshot_batch_size: UIntAtLeast<1>,
    #[serde(rename = "maxInFlightCommands")]
    pub max_in_flight_commands: UIntAtLeast<1>,
    #[serde(rename = "maxPendingQueueBytes")]
    pub max_pending_queue_bytes: UIntAtLeast<1024>,
    #[serde(rename = "maxPendingQueueMessages")]
    pub max_pending_queue_messages: UIntAtLeast<1>,
    #[serde(rename = "heartbeatIntervalMs")]
    pub heartbeat_interval_ms: BoundedU64<1000, 300_000>,
}

/// `common.schema.json#/$defs/transcriptCodec`：与 `SYNC_PROTOCOL.md` §6.2 同一二进制 codec。
pub const TRANSCRIPT_CODEC: &str = "acpr-transcript-v1";

/// `common.schema.json#/$defs/transcriptDomain`：Node Link 的六个 domain。
///
/// 取值必须与 `crate::domains::DOMAINS` 的表逐条一致（由契约测试断言）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TranscriptDomain {
    PairingProof,
    PairingOwnerProof,
    PairingSas,
    PairingStatus,
    Challenge,
    Proof,
}

impl TranscriptDomain {
    pub const ALL: [TranscriptDomain; 6] = [
        TranscriptDomain::PairingProof,
        TranscriptDomain::PairingOwnerProof,
        TranscriptDomain::PairingSas,
        TranscriptDomain::PairingStatus,
        TranscriptDomain::Challenge,
        TranscriptDomain::Proof,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TranscriptDomain::PairingProof => "acp-remote/node-link-pairing-proof/v1",
            TranscriptDomain::PairingOwnerProof => "acp-remote/node-link-pairing-owner-proof/v1",
            TranscriptDomain::PairingSas => "acp-remote/node-link-pairing-sas/v1",
            TranscriptDomain::PairingStatus => "acp-remote/node-link-pairing-status/v1",
            TranscriptDomain::Challenge => "acp-remote/node-link-challenge/v1",
            TranscriptDomain::Proof => "acp-remote/node-link-proof/v1",
        }
    }
}

impl fmt::Display for TranscriptDomain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for TranscriptDomain {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        TranscriptDomain::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "domain",
                value: text.to_owned(),
            })
    }
}

impl Serialize for TranscriptDomain {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TranscriptDomain {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        TranscriptDomain::from_str(&text).map_err(DeError::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_strings_reject_out_of_domain_values() {
        assert!(ExportId::parse("export-laptop-zed").is_ok());
        assert!(ExportId::parse("").is_err());
        assert!(ExportId::parse("has space").is_err());
        assert!(ExportId::parse("has/slash").is_err());
        assert!(ExportId::parse(&"a".repeat(129)).is_err());

        assert!(WorkspaceAlias::parse("default").is_ok());
        assert!(WorkspaceAlias::parse("a").is_ok());
        assert!(WorkspaceAlias::parse("A").is_err(), "必须小写开头");
        assert!(WorkspaceAlias::parse("_leading").is_err());
        assert!(WorkspaceAlias::parse("/abs/path").is_err(), "别名不是路径");
        assert!(WorkspaceAlias::parse(&format!("a{}", "b".repeat(64))).is_err());

        assert!(TemplateParamName::parse("_private1").is_ok());
        assert!(TemplateParamName::parse("1leading").is_err());
        assert!(TemplateParamName::parse("has-dash").is_err());

        assert!(OpaqueRef::parse("ceiling-1").is_ok());
        assert!(TemplateId::parse("tmpl.v1").is_ok());
        assert!(AgentId::parse("omp").is_ok());
    }

    #[test]
    fn grant_list_rejects_duplicates_and_keeps_the_commands_json_vocabulary() {
        assert!(GrantList::new(vec![GrantName::Observe, GrantName::Interact]).is_ok());
        assert!(
            GrantList::new(vec![GrantName::Observe, GrantName::Observe]).is_err(),
            "uniqueItems 必须被执行"
        );
        let names: Vec<&str> = GrantName::ALL.iter().map(|grant| grant.as_str()).collect();
        assert_eq!(
            names,
            [
                "grant.observe",
                "grant.interact",
                "grant.configure-session",
                "grant.approve",
                "grant.remote-work"
            ]
        );
    }

    #[test]
    fn export_entry_caps_are_enforced() {
        let agent = ExportAgent {
            agent_id: AgentId::parse("omp").expect("agentId"),
            name: NonEmptyText::parse("Oh My Pi").expect("name"),
            capabilities_ref: OpaqueRef::parse("ceiling-1").expect("opaqueRef"),
        };
        let entry = ExportEntry {
            export_id: ExportId::parse("export-laptop-zed").expect("exportId"),
            display_name: NonEmptyText::parse("Laptop").expect("name"),
            agents: vec![agent; MAX_EXPORT_AGENTS + 1],
            workspace_aliases: Vec::new(),
            default_workspace_alias: WorkspaceAlias::parse("default").expect("alias"),
            templates: Vec::new(),
            scopes: GrantList::new(vec![GrantName::Observe]).expect("grants"),
            capability_ceiling_ref: OpaqueRef::parse("ceiling-1").expect("opaqueRef"),
            cache_policy: NoContentCache,
            revoked: false,
        };
        let serialized = serde_json::to_string(&entry).expect("可序列化");
        let decoded = serde_json::from_str::<ExportEntry>(&serialized);
        assert!(
            decoded.is_err(),
            "agents 的 maxItems 1024 必须在反序列化期被执行"
        );
    }

    #[test]
    fn template_param_values_and_caps_are_enforced() {
        assert!(TemplateParamValues::new(vec!["a".to_owned()]).is_ok());
        assert!(TemplateParamValues::new(Vec::new()).is_err(), "minItems 1");
        assert!(
            TemplateParamValues::new(vec!["a".to_owned(); 257]).is_err(),
            "maxItems 256"
        );

        let param = WorkspaceTemplateParam {
            name: TemplateParamName::parse("mode").expect("name"),
            param_type: TemplateParamType::String,
            required: true,
            pattern: Nullable::null(),
            values: Nullable::value(
                TemplateParamValues::new(vec!["fast".to_owned()]).expect("values"),
            ),
        };
        let template = WorkspaceTemplate {
            template_id: TemplateId::parse("tmpl.v1").expect("templateId"),
            display_name: NonEmptyText::parse("Default").expect("name"),
            workspace_alias: WorkspaceAlias::parse("default").expect("alias"),
            params: vec![param; MAX_TEMPLATE_PARAMS + 1],
        };
        let serialized = serde_json::to_string(&template).expect("可序列化");
        assert!(
            serde_json::from_str::<WorkspaceTemplate>(&serialized).is_err(),
            "params 的 maxItems 32 必须被执行"
        );
    }

    #[test]
    fn limits_bounds_are_enforced() {
        let limits = |heartbeat: u64| {
            format!(
                "{{\"maxMessageBytes\":1048576,\"catalogSnapshotBatchSize\":64,\"resourceSnapshotBatchSize\":64,\"maxInFlightCommands\":16,\"maxPendingQueueBytes\":1048576,\"maxPendingQueueMessages\":256,\"heartbeatIntervalMs\":{heartbeat}}}"
            )
        };
        assert!(serde_json::from_str::<NodeLinkLimits>(&limits(30_000)).is_ok());
        assert!(serde_json::from_str::<NodeLinkLimits>(&limits(999)).is_err());
        assert!(serde_json::from_str::<NodeLinkLimits>(&limits(300_001)).is_err());

        let too_small =
            limits(30_000).replace("\"maxMessageBytes\":1048576", "\"maxMessageBytes\":1023");
        assert!(serde_json::from_str::<NodeLinkLimits>(&too_small).is_err());
    }

    #[test]
    fn transcript_domains_match_the_table() {
        let from_table: Vec<&str> = crate::domains::DOMAINS
            .iter()
            .map(|domain| domain.domain)
            .collect();
        let from_enum: Vec<&str> = TranscriptDomain::ALL
            .iter()
            .map(|domain| domain.as_str())
            .collect();
        assert_eq!(
            from_enum, from_table,
            "domain 枚举必须与 transcript 表逐条相等"
        );
        assert_eq!(TRANSCRIPT_CODEC, "acpr-transcript-v1");
    }
}
