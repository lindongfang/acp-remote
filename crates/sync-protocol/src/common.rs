//! Sync v1 的公共类型（`schemas/sync/v1/common.schema.json` 的 `$defs`）里 sync 专属的部分。
//!
//! 与协议无关的 wire 值对象——`uuid`、`decimalString`、`featureId`/`featureList`、
//! `base64url*`、`timestamp`、`rawAcp`、有界整数、长度受限文本、`T | null` 字段、
//! `publicError` 的形状、原始 JSON 对象与未知字段载体——由叶子 crate `acpr-wire` 拥有，本模块
//! 只做命名空间再导出（见下面的 `pub use`），既有路径 `crate::common::Uuid` 保持不变。
//!
//! 留在这里的是 sync 线自己的形状：`cursor`、`originBlock`/`remoteOrigin`，以及从它们派生的
//! 会话读数（`modeRef`/`modeState`、`configOptionView`、`agentContentBlock`、
//! `interactionOption`、`promptContentBlock`、`sessionSummary`）；错误码词表见
//! [`crate::error::ErrorCode`]。
//!
//! 校验只发生在反序列化：构造出来的值必然满足对应 schema，后续层不需要重复校验。

use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ErrorCode;

/// 值对象与机制来自叶子 crate `acpr-wire`，此处只做命名空间再导出：`crate::common::Uuid`、
/// `sync_protocol::common::Base64Url` 等既有路径因此保持不变。
pub use acpr_wire::{
    Base64Url, BoundedU64, DecimalString, ExtraFields, FeatureId, FeatureList, NonEmptyText,
    Nullable, ProtocolVersionV1, RawAcp, RawAcpUnavailableReason, RawObject, Text, Timestamp,
    UIntAtLeast, Uuid, ValueError, deserialize_optional_non_null,
};

/// schema 里"键可缺失、出现时可为 `null`"的字段的 `deserialize_with`，配
/// `#[serde(default, deserialize_with = "...")]` 使用。
///
/// 单独一个 `Option<Nullable<T>>` 不足以表达三态：serde 的 `Option` 在 `null` 上直接返回
/// `None`，于是"键缺席"与"键在但为 `null`"都被压成同一个值，往返时会把显式 `null` 的键丢掉。
/// 这里让"键在不在"由外层 `default` 负责、"值是不是 `null`"由 [`Nullable`] 负责——因此
/// `None` 只在键缺席时产生。
pub fn deserialize_optional_nullable<'de, D, T>(
    deserializer: D,
) -> Result<Option<Nullable<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    Nullable::<T>::deserialize(deserializer).map(Some)
}

/// `common.schema.json#/$defs/publicError`：跨消息引用的公开错误形状，四个键都必需。
///
/// 形状与校验来自 [`acpr_wire::PublicError`]；`code` 绑定的词表是 sync 线的 [`ErrorCode`]。
pub type PublicError = acpr_wire::PublicError<ErrorCode>;

/// `cursor`：`common.schema.json#/$defs/cursor`，`serverEpoch` 与 `globalSequence` 都必需。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    #[serde(rename = "serverEpoch")]
    pub server_epoch: Uuid,
    #[serde(rename = "globalSequence")]
    pub global_sequence: DecimalString,
}

/// `common.schema.json#/$defs/originCursor`：Owner 来源日志的位置（`resource.stateless-origin.v1`）。
///
/// 这是**来源**的序号域，与 [`Cursor`] 的 Access 本机序号无关（design D3）：远程流直接以
/// `(originEpoch, originSequence)` 订阅与 ACK，不为 imported 事件分配 Access 本地 `globalSequence`。
/// Node Link 在自己的 crate 里定义同名同形类型，两个协议 crate 不互相依赖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginCursor {
    #[serde(rename = "originEpoch")]
    pub origin_epoch: Uuid,
    #[serde(rename = "originSequence")]
    pub origin_sequence: DecimalString,
}

/// `common.schema.json#/$defs/sessionRef`：Owner 侧会话的跨节点引用。
///
/// `(ownerNodeId, sessionId)` 共同构成来源命名空间：客户端去重键与远程订阅都绑定它，
/// 不同 Owner 上相同的 `sessionId` 不得被合并（design D3 的风险项）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRef {
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
}

/// `common.schema.json#/$defs/messageCoverage`：单条消息的读取覆盖声明（design D4）。
///
/// `through` 等于同次读取结果的 `readCursor`；`last_delta_index` 是该消息在所返回完整内容中已覆盖的
/// 最后一个**连续**增量索引，无增量时为 `null`。它是逐条消息的合并依据，不声明整页或整个浏览是同一
/// 历史快照：下一页有自己的 `readCursor`，迟到的旧页不得覆盖更新的消息内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageCoverage {
    #[serde(rename = "messageId")]
    pub message_id: Uuid,
    pub through: OriginCursor,
    #[serde(rename = "lastDeltaIndex")]
    pub last_delta_index: Nullable<DecimalString>,
}

/// `common.schema.json#/$defs/originBlock`：会话/事件的归属，`kind` 是判别标签。
///
/// 两个分支都是 `additionalProperties: false`（`local` 分支只允许 `kind`）。serde 的内部标签
/// 表示对**单元变体**不做未知字段检查（单元变体没有字段表可拒），所以这里手写反序列化，
/// 保持 `OriginBlock::Local` 这个契约里的变体形状不变。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum OriginBlock {
    /// `{"kind": "local"}`：会话由本节点拥有。
    Local,
    /// `{"kind": "remote", …}`：会话由别的节点拥有，本节点只持有投影。
    #[serde(rename = "remote")]
    Remote {
        #[serde(rename = "ownerNodeId")]
        owner_node_id: Uuid,
        #[serde(rename = "originEpoch")]
        origin_epoch: Uuid,
        online: bool,
    },
}

impl<'de> Deserialize<'de> for OriginBlock {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OriginBlockVisitor;

        impl<'de> serde::de::Visitor<'de> for OriginBlockVisitor {
            type Value = OriginBlock;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("{\"kind\":\"local\"} 或 remote 归属对象")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<Self::Value, A::Error> {
                let mut kind: Option<String> = None;
                let mut owner_node_id: Option<Uuid> = None;
                let mut origin_epoch: Option<Uuid> = None;
                let mut online: Option<bool> = None;
                while let Some(key) = access.next_key::<String>()? {
                    match key.as_str() {
                        "kind" => {
                            if kind.is_some() {
                                return Err(DeError::duplicate_field("kind"));
                            }
                            kind = Some(access.next_value()?);
                        }
                        "ownerNodeId" => {
                            if owner_node_id.is_some() {
                                return Err(DeError::duplicate_field("ownerNodeId"));
                            }
                            owner_node_id = Some(access.next_value()?);
                        }
                        "originEpoch" => {
                            if origin_epoch.is_some() {
                                return Err(DeError::duplicate_field("originEpoch"));
                            }
                            origin_epoch = Some(access.next_value()?);
                        }
                        "online" => {
                            if online.is_some() {
                                return Err(DeError::duplicate_field("online"));
                            }
                            online = Some(access.next_value()?);
                        }
                        other => {
                            return Err(DeError::unknown_field(
                                other,
                                &["kind", "ownerNodeId", "originEpoch", "online"],
                            ));
                        }
                    }
                }

                match kind.as_deref() {
                    Some("local") => {
                        if owner_node_id.is_some() || origin_epoch.is_some() || online.is_some() {
                            return Err(DeError::custom(ValueError::Shape {
                                field: "originBlock",
                                expected: "local 分支只允许 kind",
                            }));
                        }
                        Ok(OriginBlock::Local)
                    }
                    Some("remote") => Ok(OriginBlock::Remote {
                        owner_node_id: owner_node_id
                            .ok_or_else(|| DeError::missing_field("ownerNodeId"))?,
                        origin_epoch: origin_epoch
                            .ok_or_else(|| DeError::missing_field("originEpoch"))?,
                        online: online.ok_or_else(|| DeError::missing_field("online"))?,
                    }),
                    Some(other) => Err(DeError::custom(ValueError::Enumerated {
                        field: "originBlock.kind",
                        value: other.to_owned(),
                    })),
                    None => Err(DeError::missing_field("kind")),
                }
            }
        }

        deserializer.deserialize_map(OriginBlockVisitor)
    }
}

/// `common.schema.json#/$defs/remoteOrigin`：远端事件的会话归属与原始序号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteOrigin {
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "originEpoch")]
    pub origin_epoch: Uuid,
    #[serde(rename = "originEventId")]
    pub origin_event_id: Uuid,
    #[serde(rename = "originSequence")]
    pub origin_sequence: DecimalString,
}

/// `common.schema.json#/$defs/modeRef`：一个可选的会话模式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeRef {
    #[serde(rename = "modeId")]
    pub mode_id: NonEmptyText<256>,
    #[serde(rename = "displayName")]
    pub display_name: NonEmptyText<256>,
}

/// `common.schema.json#/$defs/modeState`：`currentModeId` 是 required 且可 `null`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeState {
    #[serde(rename = "currentModeId")]
    pub current_mode_id: Nullable<NonEmptyText<256>>,
    #[serde(rename = "availableModes")]
    pub available_modes: Vec<ModeRef>,
    pub version: DecimalString,
}

/// `configOptionView.type`：`select | boolean`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigOptionType {
    Select,
    Boolean,
}

/// `configOptionView.currentValue`：`string | boolean`（schema 是 `type: ["string","boolean"]`，
/// 没有长度限制，所以 string 分支保存原样的 `String`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigOptionValue {
    Text(String),
    Boolean(bool),
}

impl Serialize for ConfigOptionValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Text(text) => serializer.serialize_str(text),
            Self::Boolean(flag) => serializer.serialize_bool(*flag),
        }
    }
}

impl<'de> Deserialize<'de> for ConfigOptionValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::String(text) => Ok(Self::Text(text)),
            serde_json::Value::Bool(flag) => Ok(Self::Boolean(flag)),
            _ => Err(DeError::custom(ValueError::Shape {
                field: "configOptionView.currentValue",
                expected: "\"string\" | \"boolean\"",
            })),
        }
    }
}

/// `configOptionView.options` 的元素（schema 里是内联对象，`value`/`name`/`description` 都必需）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigOption {
    pub value: NonEmptyText<256>,
    pub name: NonEmptyText<256>,
    pub description: Nullable<Text<1024>>,
}

/// `common.schema.json#/$defs/configOptionView`：ACP `SessionConfigOption` 的公开投影。
///
/// `options` 不在 `required` 里（键可以缺失）；`description`/`category` 是 required 且可 `null`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigOptionView {
    pub id: NonEmptyText<256>,
    pub name: NonEmptyText<256>,
    pub description: Nullable<Text<1024>>,
    pub category: Nullable<Text<128>>,
    #[serde(rename = "type")]
    pub option_type: ConfigOptionType,
    #[serde(rename = "currentValue")]
    pub current_value: ConfigOptionValue,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub options: Option<Vec<ConfigOption>>,
}

/// `common.schema.json#/$defs/agentContentBlock` 的 `displayState`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayState {
    Available,
    NotFetched,
    Unsupported,
}

/// `agentContentBlock.unsupported.reason`：只有 `unsupported_by_client` 一个取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedByClient {
    UnsupportedByClient,
}

/// `file_ref.displayState`：文件引用没有「未获取」态（它必然指向已封存附件），过期是显式状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileDisplayState {
    /// 可读取。
    Available,
    /// 超出保留期或已随消息清理。
    Expired,
    /// 当前端到端能力不具备该类型。
    Unsupported,
}

/// `common.schema.json#/$defs/agentContentBlock`：六个以 `type` 判别的互斥分支，
/// 每个分支都是 `additionalProperties: false`，所以用带 `deny_unknown_fields` 的内部标签表示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum AgentContentBlock {
    /// `{"type": "text", "text": …}`（`text` 允许空串，`maxLength` 262144）。
    #[serde(rename = "text")]
    Text { text: Text<262144> },
    /// `{"type": "image_ref", …}`：只带引用与摘要，图片本体不在这里。
    #[serde(rename = "image_ref")]
    ImageRef {
        #[serde(rename = "mimeType")]
        mime_type: NonEmptyText<128>,
        #[serde(rename = "byteLength")]
        byte_length: Nullable<DecimalString>,
        #[serde(rename = "displayState")]
        display_state: DisplayState,
        /// Owner 已抽取的嵌入字节对应的附件标识；只有外部 URI 或未取得内容时缺失或为 `null`。
        #[serde(
            rename = "attachmentId",
            default,
            deserialize_with = "deserialize_optional_nullable",
            skip_serializing_if = "Option::is_none"
        )]
        attachment_id: Option<Nullable<Uuid>>,
    },
    /// `{"type": "audio_ref", …}`：音频引用（`core.typed-media.v1`）。
    #[serde(rename = "audio_ref")]
    AudioRef {
        #[serde(rename = "mimeType")]
        mime_type: NonEmptyText<128>,
        #[serde(rename = "byteLength")]
        byte_length: Nullable<DecimalString>,
        #[serde(rename = "displayState")]
        display_state: DisplayState,
        #[serde(
            rename = "attachmentId",
            default,
            deserialize_with = "deserialize_optional_nullable",
            skip_serializing_if = "Option::is_none"
        )]
        attachment_id: Option<Nullable<Uuid>>,
    },
    /// `{"type": "file_ref", …}`：必然指向已封存附件（`core.typed-media.v1`）。
    #[serde(rename = "file_ref")]
    FileRef {
        #[serde(rename = "attachmentId")]
        attachment_id: Uuid,
        #[serde(rename = "mimeType")]
        mime_type: NonEmptyText<255>,
        #[serde(rename = "byteLength")]
        byte_length: DecimalString,
        #[serde(rename = "displayName")]
        display_name: Nullable<NonEmptyText<255>>,
        #[serde(rename = "displayState")]
        display_state: FileDisplayState,
    },
    /// `{"type": "resource_ref", …}`：资源引用。
    #[serde(rename = "resource_ref")]
    ResourceRef {
        uri: NonEmptyText<2048>,
        name: Nullable<Text<512>>,
        #[serde(rename = "displayState")]
        display_state: DisplayState,
    },
    /// `{"type": "unsupported", …}`：本客户端不支持的内容块。
    #[serde(rename = "unsupported")]
    Unsupported {
        #[serde(rename = "originalType")]
        original_type: NonEmptyText<128>,
        reason: UnsupportedByClient,
    },
}

/// `common.schema.json#/$defs/interactionOption`：一次交互请求中的可选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionOption {
    #[serde(rename = "optionId")]
    pub option_id: NonEmptyText<128>,
    pub label: NonEmptyText<512>,
    pub kind: NonEmptyText<64>,
}

/// `promptContentBlock` 的会话工作目录内相对路径（`workspace_path_ref.path`）。
///
/// wire pattern 只拒绝前导 `/`、任意冒号、`.`/`..` 分量与控制字符。实现额外拒绝反斜杠：在 Windows 上
/// 它同样是路径分隔符，`..\\..` 会绕过只按 `/` 分段的 pattern —— 这是安全判定，不是第二套 wire 词表
/// （schema 允许的路径仍然全部接受）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePath(String);

impl WorkspacePath {
    /// 校验并构造。
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let length = text.chars().count();
        if length == 0 || length > 4096 {
            return Err(ValueError::TextLength {
                min: 1,
                max: 4096,
                actual: length,
            });
        }
        let invalid = text.starts_with('/')
            || text.contains(':')
            || text.contains('\\')
            || text.chars().any(|ch| (ch as u32) < 0x20);
        if invalid {
            return Err(ValueError::Pattern {
                field: "path",
                value: text.to_owned(),
            });
        }
        for component in text.split('/') {
            if component == "." || component == ".." {
                return Err(ValueError::Pattern {
                    field: "path",
                    value: text.to_owned(),
                });
            }
        }
        Ok(Self(text.to_owned()))
    }

    /// 原样文本。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for WorkspacePath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for WorkspacePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/promptContentBlock`：三个封闭分支。
///
/// `text` 始终可用；`attachment_ref` 只在协商 `core.attachment-transfer.v1` 且附件已封存、已绑定本
/// 会话/主体时接受；`workspace_path_ref` 只表示当前会话工作目录内的相对路径，不固定版本、不追踪内容
/// 变化。未协商时新分支必须缺席（门控在服务端适配器，形状在这里）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum PromptContentBlock {
    /// `{"type": "text", "text": …}`
    #[serde(rename = "text")]
    Text { text: NonEmptyText<262144> },
    /// `{"type": "attachment_ref", …}`：指向已封存附件的引用，不是内容快照。
    #[serde(rename = "attachment_ref")]
    AttachmentRef {
        #[serde(rename = "attachmentId")]
        attachment_id: Uuid,
        #[serde(rename = "mediaType")]
        media_type: NonEmptyText<255>,
        #[serde(rename = "byteLength")]
        byte_length: DecimalString,
        sha256: Base64Url<32>,
    },
    /// `{"type": "workspace_path_ref", "path": …}`：当前会话工作目录内的相对路径。
    #[serde(rename = "workspace_path_ref")]
    WorkspacePathRef { path: WorkspacePath },
}

/// [`PromptContentBlock::AttachmentRef`] 的只读字段视图。
///
/// 服务端在派发 `session.prompt` **之前**必须把每个引用绑定到已封存清单（§11.5），而绑定校验需要
/// `attachmentId`/`sha256`/`byteLength`。字段名与形状只在本 crate 有一份：适配器通过本视图读取，
/// 既不重新序列化块再解析，也不会在字段改名时悄悄漏检。
#[derive(Debug, Clone, Copy)]
pub struct AttachmentRefView<'a> {
    /// 已封存附件的标识。
    pub attachment_id: &'a Uuid,
    /// 附件的媒体类型（声明值，权威元数据由服务端按标识读取）。
    pub media_type: &'a NonEmptyText<255>,
    /// 附件声明长度（十进制字符串）。
    pub byte_length: &'a DecimalString,
    /// 整段内容的声明 SHA-256（无填充 base64url）。
    pub sha256: &'a Base64Url<32>,
}

impl PromptContentBlock {
    /// `attachment_ref` 的字段视图；`text`/`workspace_path_ref` 返回 `None`。
    pub fn attachment_ref(&self) -> Option<AttachmentRefView<'_>> {
        match self {
            Self::AttachmentRef {
                attachment_id,
                media_type,
                byte_length,
                sha256,
            } => Some(AttachmentRefView {
                attachment_id,
                media_type,
                byte_length,
                sha256,
            }),
            Self::Text { .. } | Self::WorkspacePathRef { .. } => None,
        }
    }
}

/// `sessionSummary.agent`（schema 里的内联对象）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionAgentRef {
    #[serde(rename = "agentId")]
    pub agent_id: NonEmptyText<128>,
    pub name: NonEmptyText<128>,
}

/// `common.schema.json#/$defs/sessionState`：会话主状态只表达活跃程度摘要。
///
/// 取值集合与 Node Link 的 `sessionMeta.state` 完全一致（`docs/SYNC_PROTOCOL.md` §10.2）：存在有效未
/// 解决交互 → `pending`；否则存在未结束任务（含已接受的排队任务）→ `running`；否则 `idle`。排队、
/// 等待原因与任务结果**不是**本词表的取值，它们留在任务/交互/命令各自的词表里；旧七态只允许作为
/// 历史事件 `view` 的原文出现（`view` 是开放对象，不经过本枚举），MUST NOT 作为实时摘要解码。
///
/// 实时摘要的取值集合只有这一份定义：任何 `sessionSummary.state` 的消费方都必须用它，不得在别处
/// 再声明第二个实时枚举或补一张旧值映射表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionState {
    Idle,
    Running,
    Pending,
}

impl SessionState {
    pub const ALL: [SessionState; 3] = [
        SessionState::Idle,
        SessionState::Running,
        SessionState::Pending,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::Idle => "idle",
            SessionState::Running => "running",
            SessionState::Pending => "pending",
        }
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for SessionState {
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
        std::str::FromStr::from_str(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/localActivity`：owned 会话的本机活跃程度
/// （`core.local-session-lifecycle.v1`，`docs/SYNC_PROTOCOL.md` §9.8）。
///
/// 它**不是**任务状态（[`SessionState`] 才是）：只由所属节点的当前运行期事实归约，并且只在协商了
/// 该 feature 的连接上、对 owned 摘要出现。未协商与 imported 摘要都不得携带该键（不是 `null`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalActivity {
    Idle,
    Active,
    Pending,
}

impl LocalActivity {
    /// schema 的 `enum` 顺序。
    pub const ALL: [LocalActivity; 3] = [
        LocalActivity::Idle,
        LocalActivity::Active,
        LocalActivity::Pending,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            LocalActivity::Idle => "idle",
            LocalActivity::Active => "active",
            LocalActivity::Pending => "pending",
        }
    }
}

impl std::fmt::Display for LocalActivity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for LocalActivity {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        LocalActivity::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "localActivity",
                value: text.to_owned(),
            })
    }
}

impl Serialize for LocalActivity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LocalActivity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        std::str::FromStr::from_str(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/workspaceRef`：已记录目录的**引用**（会话摘要用）。
///
/// wire 上只有不透明目录标识。规范化路径**不在**这里重复：它只出现在快照 `workspaces` 资源的条目上
/// （[`WorkspaceCatalogEntry`]，`docs/SYNC_PROTOCOL.md` §9.4/§10.3），需要它的客户端按标识在同一
/// 份快照里查表。客户端 MUST NOT 解析、拼接或由标识反推路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRef {
    #[serde(rename = "directoryId")]
    pub directory_id: NonEmptyText<32>,
}

/// `common.schema.json#/$defs/workspaceCatalogEntry`：快照 `workspaces` 资源的元素。
///
/// `path` 是已记录目录的规范化权威值（`canonicalize` 的结果剥掉平台 verbatim 前缀），与
/// `directoryId` **成对**下发；客户端 MUST NOT 拼接、规范化或反查它。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceCatalogEntry {
    #[serde(rename = "directoryId")]
    pub directory_id: NonEmptyText<32>,
    /// 已记录目录的规范化路径（始终在场）。
    pub path: NonEmptyText<4096>,
}

/// `common.schema.json#/$defs/agentCatalogEntry`：本机已配置 Agent 的目录条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCatalogEntry {
    #[serde(rename = "agentId")]
    pub agent_id: NonEmptyText<128>,
    #[serde(rename = "displayName")]
    pub display_name: NonEmptyText<128>,
    pub default: bool,
}

/// `common.schema.json#/$defs/sessionSummary`：会话列表/快照里的会话投影。
///
/// `title` 与 `currentMode` 是 required 且可 `null`（见 [`Nullable`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSummary {
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub title: Nullable<Text<512>>,
    pub agent: SessionAgentRef,
    pub state: SessionState,
    /// 本机活跃程度（`core.local-session-lifecycle.v1`），三态：`None` = 键**缺席**（未协商该
    /// feature，或该摘要属于 imported 会话）；`Some(..)` = 键在场。它不可为 `null`，因此用
    /// [`deserialize_optional_non_null`] 而不是可空包装。
    #[serde(
        rename = "localActivity",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub local_activity: Option<LocalActivity>,
    pub origin: OriginBlock,
    #[serde(rename = "currentMode")]
    pub current_mode: Nullable<ModeRef>,
    pub version: DecimalString,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    #[serde(rename = "updatedAt")]
    pub updated_at: Timestamp,
    /// 目录归属引用（`core.local-catalog.v1` 门控下的**可选**字段），三态：
    ///
    /// - `None`：键**缺席**。未协商该 feature 时必须是这个形状——schema 是
    ///   `additionalProperties: false`，发 `null` 会让未协商的旧客户端拒绝整条摘要，所以该字段
    ///   不在 `required` 中，且 `None` 被 `skip_serializing_if` 跳过。
    /// - `Some(Nullable::null())`：键在、值为 `null`，表示「未分组」。
    /// - `Some(Nullable::value(..))`：键在、带引用。
    ///
    /// imported（远程来源）会话固定投影为 `Some(Nullable::null())`，不得用本机 workspace
    /// 集合为它猜测归属。
    #[serde(
        rename = "workspace",
        default,
        deserialize_with = "deserialize_optional_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub workspace: Option<Nullable<WorkspaceRef>>,
}

/// `common.schema.json#/$defs/positiveDecimalString`：`1..=2^63−1` 的无前导零十进制字符串。
///
/// 准备运行期资源代 `generation` 的编码（`SYNC_PROTOCOL.md` §11.9）。与 [`DecimalString`] 不同，
/// 它不接受 `0`；上界由服务端比较判定，wire 形状只保证「正、无前导零、纯 ASCII 数字」。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PositiveDecimalString(String);

impl PositiveDecimalString {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let bytes = text.as_bytes();
        let valid = !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit) && bytes[0] != b'0';
        if !valid {
            return Err(ValueError::DecimalString(text.to_owned()));
        }
        // 上界 `2^63−1`：`i64::MAX` 的十进制文本按长度/字典序比较即可（无前导零、纯数字）。
        let limit = i64::MAX.to_string();
        if bytes.len() > limit.len() || (bytes.len() == limit.len() && text > limit.as_str()) {
            return Err(ValueError::DecimalString(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for PositiveDecimalString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PositiveDecimalString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/preparationWorkspaceSource`：准备凭据的目录来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreparationWorkspaceSource {
    /// `directoryId`：目标来自本机已登记的目录标识。
    DirectoryId,
    /// `workspacePath`：目标来自客户端提交的绝对路径。
    WorkspacePath,
}

impl PreparationWorkspaceSource {
    pub const ALL: [PreparationWorkspaceSource; 2] = [
        PreparationWorkspaceSource::DirectoryId,
        PreparationWorkspaceSource::WorkspacePath,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PreparationWorkspaceSource::DirectoryId => "directoryId",
            PreparationWorkspaceSource::WorkspacePath => "workspacePath",
        }
    }
}

impl fmt::Display for PreparationWorkspaceSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PreparationWorkspaceSource {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "workspaceSource",
                value: text.to_owned(),
            })
    }
}

impl Serialize for PreparationWorkspaceSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PreparationWorkspaceSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_str(&text).map_err(DeError::custom)
    }
}

/// `common.schema.json#/$defs/preparationReceiptClaims`：准备凭据的闭合 claims（design D2）。
///
/// 只证明「该 `sessionId` 由当前 Owner 分配并绑定到该设备/标签页/Agent/目录」；不是登录凭证、
/// 不授予 scope、不含文字/附件/Provider/MCP 凭据。`storeEpoch` 是持久 `meta.server_epoch`，
/// `tabNonce` 是 32 字节标签页归属。字段顺序即 transcript domain
/// `acp-remote/session-preparation/v1` 的 tag 顺序（`SYNC_PROTOCOL.md` §6.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationReceiptClaims {
    pub version: ProtocolVersionV1,
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    #[serde(rename = "storeEpoch")]
    pub store_epoch: Uuid,
    pub origin: NonEmptyText<255>,
    #[serde(rename = "deviceId")]
    pub device_id: Uuid,
    #[serde(rename = "tabNonce")]
    pub tab_nonce: Base64Url<32>,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    #[serde(rename = "agentId")]
    pub agent_id: NonEmptyText<128>,
    #[serde(rename = "normalizedWorkspacePath")]
    pub normalized_workspace_path: NonEmptyText<4096>,
    #[serde(rename = "workspaceSource")]
    pub workspace_source: PreparationWorkspaceSource,
}

/// `common.schema.json#/$defs/preparationReceipt`：闭合 claims 加 P1363 64 字节签名（base64url）。
///
/// 签名域是 `acp-remote/session-preparation/v1` 的 transcript，不接受普通 JSON 签名；本类型只承载
/// wire 形状，签发/验签由 `identity-auth` 的专用纯类型入口完成。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationReceipt {
    pub claims: PreparationReceiptClaims,
    pub signature: Base64Url<64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_requires_both_keys_and_rejects_unknown_and_ill_typed() {
        let cursor: Cursor = serde_json::from_str(
            r#"{"serverEpoch":"00384a03-bc90-4095-b65d-82fb8cc47e13","globalSequence":"0"}"#,
        )
        .expect("cursor 合法");
        assert_eq!(cursor.global_sequence.as_str(), "0");

        for rejected in [
            // 两个键都是 required。
            r#"{"serverEpoch":"00384a03-bc90-4095-b65d-82fb8cc47e13"}"#,
            r#"{"globalSequence":"0"}"#,
            // additionalProperties: false。
            r#"{"serverEpoch":"00384a03-bc90-4095-b65d-82fb8cc47e13","globalSequence":"0","extra":1}"#,
            // 类型与取值域。
            r#"{"serverEpoch":"00384a03-bc90-4095-b65d-82fb8cc47e13","globalSequence":0}"#,
            r#"{"serverEpoch":"00384A03-BC90-4095-B65D-82FB8CC47E13","globalSequence":"0"}"#,
            r#"{"serverEpoch":"00384a03-bc90-4095-b65d-82fb8cc47e13","globalSequence":"00"}"#,
        ] {
            assert!(
                serde_json::from_str::<Cursor>(rejected).is_err(),
                "cursor {rejected} 应被拒绝"
            );
        }
    }

    #[test]
    fn session_state_is_the_three_state_summary() {
        assert_eq!(
            SessionState::ALL.map(SessionState::as_str),
            ["idle", "running", "pending"],
            "实时摘要只能有这三个取值"
        );

        for state in SessionState::ALL {
            let text = state.as_str();
            let encoded = serde_json::to_string(&state).expect("可序列化");
            assert_eq!(encoded, format!("\"{text}\""));
            assert_eq!(
                serde_json::from_str::<SessionState>(&encoded).expect("往返可解码"),
                state
            );
            assert_eq!(
                text.parse::<SessionState>()
                    .expect("Display 与 FromStr 互逆"),
                state
            );
        }

        // 旧七态只允许留在历史事件 `view` 的原文里；作为实时摘要解码必须被拒。
        for legacy in [
            "queued",
            "waiting_input",
            "waiting_permission",
            "failed",
            "closed",
        ] {
            assert!(
                serde_json::from_str::<SessionState>(&format!("\"{legacy}\"")).is_err(),
                "{legacy} 不再是实时会话状态"
            );
            assert!(
                legacy.parse::<SessionState>().is_err(),
                "{legacy} 不得被解析"
            );
        }
    }
}
