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

use serde::de::Error as DeError;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ErrorCode;

/// 值对象与机制来自叶子 crate `acpr-wire`，此处只做命名空间再导出：`crate::common::Uuid`、
/// `sync_protocol::common::Base64Url` 等既有路径因此保持不变。
pub use acpr_wire::{
    Base64Url, BoundedU64, DecimalString, ExtraFields, FeatureId, FeatureList, NonEmptyText,
    Nullable, ProtocolVersionV1, RawAcp, RawAcpUnavailableReason, RawObject, Text, Timestamp,
    UIntAtLeast, Uuid, ValueError, deserialize_optional_non_null,
};

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
        #[serde(rename = "exportId")]
        export_id: NonEmptyText<256>,
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
                let mut export_id: Option<NonEmptyText<256>> = None;
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
                        "exportId" => {
                            if export_id.is_some() {
                                return Err(DeError::duplicate_field("exportId"));
                            }
                            export_id = Some(access.next_value()?);
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
                                &["kind", "ownerNodeId", "exportId", "originEpoch", "online"],
                            ));
                        }
                    }
                }

                match kind.as_deref() {
                    Some("local") => {
                        if owner_node_id.is_some()
                            || export_id.is_some()
                            || origin_epoch.is_some()
                            || online.is_some()
                        {
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
                        export_id: export_id.ok_or_else(|| DeError::missing_field("exportId"))?,
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
    #[serde(rename = "exportId")]
    pub export_id: NonEmptyText<256>,
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

/// `agentContentBlock` 的 `unsupported.reason`：只有 `unsupported_by_client` 一个取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedByClient {
    UnsupportedByClient,
}

/// `common.schema.json#/$defs/agentContentBlock`：四个以 `type` 判别的互斥分支，
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

/// `common.schema.json#/$defs/promptContentBlock`：目前只有 `{"type": "text", "text": …}`，
/// `type` 的 `const` 在这里校验后才对外暴露（对外只有 `text` 一个字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptContentBlock {
    pub text: NonEmptyText<262144>,
}

impl Serialize for PromptContentBlock {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("type", "text")?;
        map.serialize_entry("text", &self.text)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for PromptContentBlock {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// `promptContentBlock` 的完整 wire 形状（`type` 是 `const: "text"`）。
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct PromptContentBlockWire {
            #[serde(rename = "type")]
            block_type: String,
            text: NonEmptyText<262144>,
        }

        let wire = PromptContentBlockWire::deserialize(deserializer)?;
        if wire.block_type != "text" {
            return Err(DeError::custom(ValueError::Shape {
                field: "promptContentBlock.type",
                expected: "\"text\"",
            }));
        }
        Ok(Self { text: wire.text })
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

/// `sessionSummary.state`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Queued,
    Running,
    WaitingInput,
    WaitingPermission,
    Failed,
    Closed,
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
    pub origin: OriginBlock,
    #[serde(rename = "currentMode")]
    pub current_mode: Nullable<ModeRef>,
    pub version: DecimalString,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    #[serde(rename = "updatedAt")]
    pub updated_at: Timestamp,
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
}
