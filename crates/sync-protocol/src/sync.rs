//! `sync.*` 家族 body（`schemas/sync/v1/sync.schema.json`）：游标订阅、增量补发与快照重建。
//!
//! 对应 `docs/SYNC_PROTOCOL.md` §9.2（Subscribe）、§9.3（增量重放）、§9.4（Snapshot）、§9.5（ACK）。
//! 这里只做 wire 形状与取值域校验：游标是否仍在保留窗口内、快照 digest 是否匹配、`chunkCount` 在
//! begin/end 之间是否一致这类状态判定属于会话层，本模块不做判断，只保证构造出来的值满足 schema。

use std::fmt;
use std::str::FromStr;

use serde::de::{DeserializeOwned, Error as DeError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::command::PreparationConfigSelection;
use crate::common::{
    AgentCatalogEntry, AgentContentBlock, Base64Url, BoundedU64, ConfigOptionView, Cursor,
    DecimalString, ModeRef, NonEmptyText, Nullable, OriginCursor, PositiveDecimalString,
    PreparationReceipt, ProtocolVersionV1, PublicError, RawAcp, RawObject, SessionRef,
    SessionSummary, Timestamp, Uuid, ValueError, WorkspaceCatalogEntry,
    deserialize_optional_non_null,
};
use crate::event::EventType;
use crate::media::{MessageChannel, PartialResult};

/// `sync.resetRequired.reason` / `sync.snapshotRequest.reason` 的取值（`sync.schema.json` 的两处 `enum`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResetReason {
    InitialSync,
    EpochMismatch,
    CursorExpired,
    CacheIncompatible,
}

impl ResetReason {
    pub const ALL: [ResetReason; 4] = [
        ResetReason::InitialSync,
        ResetReason::EpochMismatch,
        ResetReason::CursorExpired,
        ResetReason::CacheIncompatible,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ResetReason::InitialSync => "initial_sync",
            ResetReason::EpochMismatch => "epoch_mismatch",
            ResetReason::CursorExpired => "cursor_expired",
            ResetReason::CacheIncompatible => "cache_incompatible",
        }
    }
}

impl fmt::Display for ResetReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ResetReason {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        ResetReason::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "reason",
                value: text.to_owned(),
            })
    }
}

impl Serialize for ResetReason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ResetReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        ResetReason::from_str(&text).map_err(DeError::custom)
    }
}

/// `sync.snapshotChunk.resource`：`sync.schema.json#/$defs/snapshotResource` 的封闭词表镜像。
///
/// 快照只承载 `sessions`/`workspaces`/`agents` 三类清单资源。六类 session-scoped 明细资源
/// `messages`/`turns`/`pending_interactions`/`config_options`/`capabilities`/`history` **不进快照**，无论会话的
/// `origin.kind` 是 `local` 还是 `remote`（`docs/SYNC_PROTOCOL.md` §9.4，design D1）；会话明细一律由
/// `session.read` 在线按会话内序号或复合游标分页取得，其元素形状仍由本模块的 [`SnapshotItemMessage`]
/// 等类型承载。
///
/// 取值集合与顺序由 `tests/schema_drift.rs::snapshot_resources_match_schema_enum` 与 schema 双向门禁。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapshotResource {
    Sessions,
    Workspaces,
    Agents,
}

impl SnapshotResource {
    pub const ALL: [SnapshotResource; 3] = [
        SnapshotResource::Sessions,
        SnapshotResource::Workspaces,
        SnapshotResource::Agents,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotResource::Sessions => "sessions",
            SnapshotResource::Workspaces => "workspaces",
            SnapshotResource::Agents => "agents",
        }
    }
}

impl fmt::Display for SnapshotResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for SnapshotResource {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        SnapshotResource::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "resource",
                value: text.to_owned(),
            })
    }
}

impl Serialize for SnapshotResource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SnapshotResource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        SnapshotResource::from_str(&text).map_err(DeError::custom)
    }
}

/// `sync.snapshotItem.messages.role` 的取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageRole {
    User,
    Agent,
}

impl MessageRole {
    pub const ALL: [MessageRole; 2] = [MessageRole::User, MessageRole::Agent];

    pub fn as_str(self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Agent => "agent",
        }
    }
}

impl fmt::Display for MessageRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for MessageRole {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        MessageRole::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "role",
                value: text.to_owned(),
            })
    }
}

impl Serialize for MessageRole {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MessageRole {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        MessageRole::from_str(&text).map_err(DeError::custom)
    }
}

/// `sync.snapshotItem.pending_interactions.kind` 的取值。
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

/// `sync.subscribe.body.scope`：v1 只有 `"machine"` 一个取值（`const` 校验）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Machine,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Machine => "machine",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Scope {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "machine" => Ok(Scope::Machine),
            other => Err(ValueError::Enumerated {
                field: "scope",
                value: other.to_owned(),
            }),
        }
    }
}

impl Serialize for Scope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Scope::from_str(&text).map_err(DeError::custom)
    }
}

/// `sync.snapshotItem.messages` 的单个元素。
///
/// `content` 的元素是 `common.schema.json#/$defs/agentContentBlock`；`turnId` 是 required 且可
/// `null`（`common.schema.json` 的 `uuid | null`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotItemMessage {
    #[serde(rename = "messageId")]
    pub message_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub role: MessageRole,
    pub content: Vec<AgentContentBlock>,
    pub status: NonEmptyText<32>,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    /// 首次输出位置；owned 为会话序号，远程来源为 Owner origin 序号。
    /// 缺席表示旧历史无法可靠恢复顺序，不允许按时间伪造该值。
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub sequence: Option<DecimalString>,
    #[serde(rename = "turnId")]
    pub turn_id: Nullable<Uuid>,
    /// 消息通道（`core.typed-media.v1`）：回复与思考分别持久化，客户端据此区分。
    ///
    /// 未协商该 feature 的连接不下发该键（旧 peer 的 wire 形状逐字不变）；语义上它描述的是
    /// `owned_message.channel`，不改变 `role` 的 user/agent 二分。
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub channel: Option<MessageChannel>,
    /// 正文完整性（`core.typed-media.v1`）：`partial` 表示已收到内容被保留但上游未完成。
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub partial: Option<PartialResult>,
}

/// `sync.snapshotItem.turns` 的单个元素。
///
/// `startedAt`/`completedAt`/`terminalError` 都是 required 且可 `null`；`state` 是 1..=32 的开放字符串。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotItemTurn {
    #[serde(rename = "turnId")]
    pub turn_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub state: NonEmptyText<32>,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    #[serde(rename = "startedAt")]
    pub started_at: Nullable<Timestamp>,
    #[serde(rename = "completedAt")]
    pub completed_at: Nullable<Timestamp>,
    #[serde(rename = "terminalError")]
    pub terminal_error: Nullable<PublicError>,
}

/// `sync.snapshotItem.pending_interactions` 的单个元素。
///
/// `schema` 在 schema 里是开放的 `{"type": "object"}`（`pendingInteraction` 的询问表单），
/// 因此用 [`RawObject`] 原样保留字节。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotItemPendingInteraction {
    #[serde(rename = "interactionId")]
    pub interaction_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub kind: InteractionKind,
    pub state: NonEmptyText<32>,
    pub schema: RawObject,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
}

/// `sync.snapshotItem.capabilities` 的单个元素。
///
/// `agentCapabilities`/`brokerAdditions` 都是 schema 的开放 `{"type": "object"}`：能力集合由各自的
/// 文档定义，wire 层只保证它是对象并保留原始字节。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotItemCapability {
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    #[serde(rename = "agentCapabilities")]
    pub agent_capabilities: RawObject,
    #[serde(rename = "brokerAdditions")]
    pub broker_additions: RawObject,
}

/// `sync.snapshotItem.history` 的单个元素：`session.read` 的 `resources.history` 的一条历史条目。
///
/// 与 `event.schema.json` 的 `body.payload` 同形：[`event_id`](Self::event_id)/[`event_type`](Self::event_type)/
/// [`session_sequence`](Self::session_sequence)/[`created_at`](Self::created_at) 定位事件，`view` 是与
/// 实时事件同一套解析入口的结构化视图（schema 的 `{"type": "object"}`，用 [`RawObject`] 原样保留
/// 字节；每个事件类型的最低字段见 [`crate::views`]），`acp` 是可选的原文或其不可用说明
/// （`common.schema.json#/$defs/rawAcp` 的两个分支，没有 `null` 分支）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotItemHistory {
    #[serde(rename = "eventId")]
    pub event_id: Uuid,
    #[serde(rename = "eventType")]
    pub event_type: EventType,
    #[serde(rename = "sessionSequence")]
    pub session_sequence: DecimalString,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    pub view: RawObject,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub acp: Option<RawAcp>,
}

/// `sync.snapshotItem.workspaces` 的单个元素。
///
/// `snapshotItem.workspaces` 在 schema 里是 `common.schema.json#/$defs/workspaceCatalogEntry` 的
/// `$ref`，因此直接复用同一个类型。它与 [`WorkspaceRef`]（会话摘要的目录引用）**不是**同一个形状：
/// 只有它带 feature `core.workspace-path.v1` 门控下的可选路径键，路径因此不会在每条会话摘要里重复。
pub type SnapshotItemWorkspace = WorkspaceCatalogEntry;

/// `sync.snapshotItem.agents` 的单个元素；同理是 `#/$defs/agentCatalogEntry` 的别名。
pub type SnapshotItemAgent = AgentCatalogEntry;

/// `sync.snapshotChunk.items`：元素类型由同一 body 的 `resource` 决定
/// （`sync.schema.json#/$defs/snapshotChunk` 的 `allOf`/`if`/`then`）。
///
/// 枚举变体与 [`SnapshotResource`] 一一对应，因此「variant 与 `resource` 不一致」的值不存在：
/// 反序列化由 [`SnapshotItems::parse`] 按 `resource` 分派，序列化只写数组本身。会话明细资源的
/// 元素类型（[`SnapshotItemMessage`] 等）不再有对应变体——它们不进快照，只经 `session.read` 返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotItems {
    Sessions(Vec<SessionSummary>),
    Workspaces(Vec<SnapshotItemWorkspace>),
    Agents(Vec<SnapshotItemAgent>),
}

impl SnapshotItems {
    /// `sync.snapshotChunk.items` 的最大长度（schema 的 `maxItems`）。
    pub const MAX_ITEMS: usize = 10_000;

    /// 按 `resource` 解析 `items` 数组。
    ///
    /// 元素形状与 `resource` 不符时返回 [`ValueError::Shape`]（该位置只允许对应 `snapshotItem.*`
    /// 定义的元素），超过 [`SnapshotItems::MAX_ITEMS`] 时返回 [`ValueError::TooManyItems`]。
    pub fn parse(resource: SnapshotResource, raw: &RawValue) -> Result<Self, ValueError> {
        Ok(match resource {
            SnapshotResource::Sessions => {
                let items = parse_items::<SessionSummary>(raw, "snapshotItem.sessions")?;
                SnapshotItems::Sessions(items)
            }
            SnapshotResource::Workspaces => {
                let items = parse_items::<SnapshotItemWorkspace>(raw, "snapshotItem.workspaces")?;
                SnapshotItems::Workspaces(items)
            }
            SnapshotResource::Agents => {
                let items = parse_items::<SnapshotItemAgent>(raw, "snapshotItem.agents")?;
                SnapshotItems::Agents(items)
            }
        })
    }

    /// 本组 `items` 对应的 `resource` 取值。
    pub fn resource(&self) -> SnapshotResource {
        match self {
            SnapshotItems::Sessions(_) => SnapshotResource::Sessions,
            SnapshotItems::Workspaces(_) => SnapshotResource::Workspaces,
            SnapshotItems::Agents(_) => SnapshotResource::Agents,
        }
    }

    /// 元素个数。
    pub fn len(&self) -> usize {
        match self {
            SnapshotItems::Sessions(items) => items.len(),
            SnapshotItems::Workspaces(items) => items.len(),
            SnapshotItems::Agents(items) => items.len(),
        }
    }

    /// 是否为空数组。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Serialize for SnapshotItems {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            SnapshotItems::Sessions(items) => items.serialize(serializer),
            SnapshotItems::Workspaces(items) => items.serialize(serializer),
            SnapshotItems::Agents(items) => items.serialize(serializer),
        }
    }
}

/// 按 `expected` 指的 `snapshotItem.*` 定义解析 `items` 数组，并施加 `maxItems`。
fn parse_items<T: DeserializeOwned>(
    raw: &RawValue,
    expected: &'static str,
) -> Result<Vec<T>, ValueError> {
    let items: Vec<T> = serde_json::from_str(raw.get()).map_err(|_| ValueError::Shape {
        field: "items",
        expected,
    })?;
    if items.len() > SnapshotItems::MAX_ITEMS {
        return Err(ValueError::TooManyItems {
            field: "items",
            max: SnapshotItems::MAX_ITEMS,
            actual: items.len(),
        });
    }
    Ok(items)
}

/// `sync.subscribe` 的 body（`sync.schema.json#/$defs/subscribe`；`docs/SYNC_PROTOCOL.md` §9.2）。
///
/// `cursor` 是 required 且可 `null`：首次同步发 `null`。`scope` 在 v1 固定为 `"machine"`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subscribe {
    pub cursor: Nullable<Cursor>,
    pub scope: Scope,
}

/// `sync.caught_up` 的 body（`sync.schema.json#/$defs/caughtUp`；`docs/SYNC_PROTOCOL.md` §9.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaughtUp {
    pub cursor: Cursor,
}

/// `sync.reset_required` 的 body（`sync.schema.json#/$defs/resetRequired`；`docs/SYNC_PROTOCOL.md` §9.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetRequired {
    pub reason: ResetReason,
    #[serde(rename = "snapshotAvailable")]
    pub snapshot_available: bool,
}

/// `sync.snapshot_request` 的 body（`sync.schema.json#/$defs/snapshotRequest`；`docs/SYNC_PROTOCOL.md` §9.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRequest {
    pub reason: ResetReason,
}

/// `sync.snapshot_begin` 的 body（`sync.schema.json#/$defs/snapshotBegin`；`docs/SYNC_PROTOCOL.md` §9.4）。
///
/// `schemaVersion` 是 `const 1`，用 [`ProtocolVersionV1`] 表达；`chunkCount` 是 `1..=3` 的整数，
/// 必须与 `sync.snapshot_end` 的 `chunkCount` 一致（一致性由会话层校验）。上界 3 等于可下发资源的
/// 种类数（[`SnapshotResource::ALL`]），因此声明的块数不可能超出实际能承载的清单类资源；未协商
/// `core.local-catalog.v1` 时只下发 `sessions`，取 1。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotBegin {
    #[serde(rename = "snapshotId")]
    pub snapshot_id: Uuid,
    pub cursor: Cursor,
    #[serde(rename = "schemaVersion")]
    pub schema_version: ProtocolVersionV1,
    #[serde(rename = "chunkCount")]
    pub chunk_count: BoundedU64<1, 3>,
}

/// `sync.snapshot_chunk` 的 body（`sync.schema.json#/$defs/snapshotChunk`；`docs/SYNC_PROTOCOL.md` §9.4）。
///
/// `chunkIndex` 是 `decimalString` 而不是整数（从 `0` 起连续递增）。`items` 的元素类型由 `resource`
/// 决定（schema 的 `allOf`/`if`/`then`），因此字段是私有的、`resource` 由 [`SnapshotItems`] 的 variant
/// 派生：不存在两者互相矛盾的值。反序列化先读 `resource`，再按它解析 `items`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotChunk {
    #[serde(rename = "snapshotId")]
    snapshot_id: Uuid,
    #[serde(rename = "chunkIndex")]
    chunk_index: DecimalString,
    resource: SnapshotResource,
    items: SnapshotItems,
}

impl SnapshotChunk {
    /// 组装 chunk；`resource` 取自 `items` 的 variant。
    pub fn new(snapshot_id: Uuid, chunk_index: DecimalString, items: SnapshotItems) -> Self {
        let resource = items.resource();
        Self {
            snapshot_id,
            chunk_index,
            resource,
            items,
        }
    }

    /// 本次快照的 id（同一次快照的 begin/chunk/end 相同）。
    pub fn snapshot_id(&self) -> &Uuid {
        &self.snapshot_id
    }

    /// 从 `0` 起连续递增的 chunk 序号（`decimalString`）。
    pub fn chunk_index(&self) -> &DecimalString {
        &self.chunk_index
    }

    /// 本 chunk 承载的资源种类，等于 [`SnapshotChunk::items`] 的 variant。
    pub fn resource(&self) -> SnapshotResource {
        self.resource
    }

    /// 本 chunk 的元素，最多 [`SnapshotItems::MAX_ITEMS`] 项。
    pub fn items(&self) -> &SnapshotItems {
        &self.items
    }
}

/// `sync.snapshot_chunk` 的 wire 形状：`items` 先按原始字节收下，再由 `resource` 决定如何解析。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkWire {
    #[serde(rename = "snapshotId")]
    snapshot_id: Uuid,
    #[serde(rename = "chunkIndex")]
    chunk_index: DecimalString,
    resource: SnapshotResource,
    items: Box<RawValue>,
}

impl<'de> Deserialize<'de> for SnapshotChunk {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ChunkWire::deserialize(deserializer)?;
        let items = SnapshotItems::parse(wire.resource, &wire.items).map_err(DeError::custom)?;
        Ok(Self {
            snapshot_id: wire.snapshot_id,
            chunk_index: wire.chunk_index,
            resource: wire.resource,
            items,
        })
    }
}

/// `sync.snapshot_end` 的 body（`sync.schema.json#/$defs/snapshotEnd`；`docs/SYNC_PROTOCOL.md` §9.4）。
///
/// `snapshotDigest` 是对各 chunk 原始字节按序计算的 32 字节摘要（算法见 §9.4），wire 上是
/// `base64url32`。`chunkCount` 的 `1..=3` 与 [`SnapshotBegin::chunk_count`] 同源：它必须等于实际
/// 下发的 chunk 数，上界等于可下发资源的种类数（[`SnapshotResource::ALL`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotEnd {
    #[serde(rename = "snapshotId")]
    pub snapshot_id: Uuid,
    pub cursor: Cursor,
    #[serde(rename = "chunkCount")]
    pub chunk_count: BoundedU64<1, 3>,
    #[serde(rename = "snapshotDigest")]
    pub snapshot_digest: Base64Url<32>,
}

/// `sync.ack` 的 body（`sync.schema.json#/$defs/ack`；`docs/SYNC_PROTOCOL.md` §9.5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ack {
    pub cursor: Cursor,
}

/// `sync.remote_reset_required.reason` 的取值（`sync.schema.json#/$defs/remoteResetRequired`）。
///
/// 复用既有 reset 词表里对**来源**有意义的三项；`initial_sync` 只描述本机 owned 日志的首次同步，
/// 远程流的首次重建走 null 订阅 + `remote_reset_required(cache_incompatible)`（design D4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteResetReason {
    EpochMismatch,
    CursorExpired,
    CacheIncompatible,
}

impl RemoteResetReason {
    pub const ALL: [RemoteResetReason; 3] = [
        RemoteResetReason::EpochMismatch,
        RemoteResetReason::CursorExpired,
        RemoteResetReason::CacheIncompatible,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RemoteResetReason::EpochMismatch => "epoch_mismatch",
            RemoteResetReason::CursorExpired => "cursor_expired",
            RemoteResetReason::CacheIncompatible => "cache_incompatible",
        }
    }
}

impl fmt::Display for RemoteResetReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RemoteResetReason {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        RemoteResetReason::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "reason",
                value: text.to_owned(),
            })
    }
}

impl Serialize for RemoteResetReason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RemoteResetReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        RemoteResetReason::from_str(&text).map_err(DeError::custom)
    }
}

/// `sync.remote_source_state.state` 的取值：来源连接事实，不是 owned 业务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteSourceStatus {
    Online,
    Offline,
    Revoked,
}

impl RemoteSourceStatus {
    pub const ALL: [RemoteSourceStatus; 3] = [
        RemoteSourceStatus::Online,
        RemoteSourceStatus::Offline,
        RemoteSourceStatus::Revoked,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RemoteSourceStatus::Online => "online",
            RemoteSourceStatus::Offline => "offline",
            RemoteSourceStatus::Revoked => "revoked",
        }
    }
}

impl fmt::Display for RemoteSourceStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RemoteSourceStatus {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        RemoteSourceStatus::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "state",
                value: text.to_owned(),
            })
    }
}

impl Serialize for RemoteSourceStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RemoteSourceStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        RemoteSourceStatus::from_str(&text).map_err(DeError::custom)
    }
}

/// `sync.remote_event.event.payload`（`sync.schema.json#/$defs/remoteEvent`）。
///
/// 与 Node Link `resource.event.payload` 同义：`view` 与 `acp` 至少出现一个。两者都没有的值不存在
/// （字段私有 + [`RemoteEventPayload::new`]），显式 `null` 被拒绝（两者都没有 `null` 分支）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawRemoteEventPayload")]
pub struct RemoteEventPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    view: Option<RawObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    acp: Option<RawAcp>,
}

impl RemoteEventPayload {
    /// 组装 payload；两者都为 `None` 时返回 [`ValueError::Shape`]。
    pub fn new(view: Option<RawObject>, acp: Option<RawAcp>) -> Result<Self, ValueError> {
        if view.is_none() && acp.is_none() {
            return Err(ValueError::Shape {
                field: "payload",
                expected: "payload 至少携带 view 或 acp 之一（SYNC_PROTOCOL §9.7）",
            });
        }
        Ok(Self { view, acp })
    }

    /// 开放视图，原样保留字节。
    pub fn view(&self) -> Option<&RawObject> {
        self.view.as_ref()
    }

    /// ACP 原文或其不可用说明。
    pub fn acp(&self) -> Option<&RawAcp> {
        self.acp.as_ref()
    }
}

/// [`RemoteEventPayload`] 的线形状：关闭形状在这里落地，两个字段都可缺失但不可为 `null`。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRemoteEventPayload {
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    view: Option<RawObject>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    acp: Option<RawAcp>,
}

impl TryFrom<RawRemoteEventPayload> for RemoteEventPayload {
    type Error = ValueError;

    fn try_from(raw: RawRemoteEventPayload) -> Result<Self, Self::Error> {
        Self::new(raw.view, raw.acp)
    }
}

/// `sync.remote_event.event`：Owner 来源事件本体。
///
/// 保留 `resource.event` 的 origin 三元组、`sessionRef`、`payloadDigest` 与 `createdAt`；**没有**
/// `globalSequence`/`sessionSequence`——imported 事件不分配 Access 本地序号（design D3）。`eventId`
/// 就是 Owner 的 `originEventId`，不创建替代标识；`payload.acp` 的原文逐字转发。`payloadDigest`
/// 是否与 payload 匹配由客户端/来源层校验，本模块只校验它是 32 字节规范 base64url。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteEventItem {
    #[serde(rename = "originEventId")]
    pub origin_event_id: Uuid,
    #[serde(rename = "originEpoch")]
    pub origin_epoch: Uuid,
    #[serde(rename = "originSequence")]
    pub origin_sequence: DecimalString,
    #[serde(rename = "sessionRef")]
    pub session_ref: SessionRef,
    #[serde(rename = "eventType")]
    pub event_type: EventType,
    #[serde(rename = "payloadDigest")]
    pub payload_digest: Base64Url<32>,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    pub payload: RemoteEventPayload,
}

/// `sync.remote_subscribe` 的 body（`sync.schema.json#/$defs/remoteSubscribe`，§9.7）。
///
/// `cursor` 是 required 且可 `null`：`null` 表示先取 Owner 当前元数据屏障 B 并由
/// `remote_reset_required(cache_incompatible)` 触发首次重建；非空表示从该 Owner origin cursor 增量重放。
/// 本消息只在 `resource.stateless-origin.v1` 协商后合法，未协商的连接收到它必须按
/// `protocol.type_unsupported` 拒绝（§5.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteSubscribe {
    #[serde(rename = "subscriptionId")]
    pub subscription_id: Uuid,
    #[serde(rename = "sessionRef")]
    pub session_ref: SessionRef,
    pub cursor: Nullable<OriginCursor>,
}

/// `sync.remote_event` 的 body（`sync.schema.json#/$defs/remoteEvent`，§9.7）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteEvent {
    #[serde(rename = "subscriptionId")]
    pub subscription_id: Uuid,
    pub event: RemoteEventItem,
}

/// `sync.remote_caught_up` 的 body（§9.7，design D4）：Owner 已完成来源重放屏障。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteCaughtUp {
    #[serde(rename = "subscriptionId")]
    pub subscription_id: Uuid,
    pub cursor: OriginCursor,
}

/// `sync.remote_ack` 的 body（§9.7，design D5）：仅本连接该订阅的运行期处理进度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteAck {
    #[serde(rename = "subscriptionId")]
    pub subscription_id: Uuid,
    pub cursor: OriginCursor,
}

/// `sync.remote_reset_required` 的 body（§9.7，design D4）。
///
/// `cursor` 只在 Owner 已验证有效 head 时给出；否则为 `null`。本消息不新造错误码。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteResetRequired {
    #[serde(rename = "subscriptionId")]
    pub subscription_id: Uuid,
    pub reason: RemoteResetReason,
    pub cursor: Nullable<OriginCursor>,
}

/// `sync.remote_unsubscribe` 的 body（§9.7）：取消本连接订阅及其观察者。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteUnsubscribe {
    #[serde(rename = "subscriptionId")]
    pub subscription_id: Uuid,
}

/// `sync.remote_unavailable` 的 body（§9.7，design D4/D6）：复用既有结构化错误。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteUnavailable {
    #[serde(rename = "subscriptionId")]
    pub subscription_id: Uuid,
    pub error: PublicError,
}

/// `sync.remote_catalog_changed` 的 body（§9.7，design D6）：提示该来源清单失效，不携带旧副本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteCatalogChanged {
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    pub revision: DecimalString,
}

/// `sync.remote_source_state` 的 body（§9.7，design D6）：来源连接事实，不写入 owned 事件流。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteSourceState {
    #[serde(rename = "ownerNodeId")]
    pub owner_node_id: Uuid,
    pub state: RemoteSourceStatus,
}

/// `viewVersion`（`sync.schema.json#/$defs/viewVersion`）：查看声明的单调版本。
///
/// 客户端从 0 开始自增；取值是结构性整数，上界 `2^53−1` 使 JavaScript 与 Rust 都能精确表示。
pub type ViewVersion = BoundedU64<0, 9_007_199_254_740_991>;

/// `session.view`（`sync.schema.json#/$defs/sessionView`）的 body（§11.8）。
///
/// `sessionId` 为 `null` = 离页；`viewVersion` 由客户端单调递增。声明只更新运行期的查看者与租约，
/// 不启动 Agent、不写事件、不改变会话状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionView {
    pub session_id: Nullable<Uuid>,
    pub view_version: ViewVersion,
}

/// `session.view.result.status`（`sync.schema.json#/$defs/sessionViewResult`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewStatus {
    /// 声明已生效并重新开始租期：必须给出**未来**的 `leaseExpiresAt`。
    Accepted,
    /// 该版本已过期/已被替换/同版本出现不同选择：`leaseExpiresAt` 必须为 `null`。
    Stale,
}

impl ViewStatus {
    pub const ALL: [ViewStatus; 2] = [ViewStatus::Accepted, ViewStatus::Stale];

    pub fn as_str(self) -> &'static str {
        match self {
            ViewStatus::Accepted => "accepted",
            ViewStatus::Stale => "stale",
        }
    }
}

impl std::fmt::Display for ViewStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for ViewStatus {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        ViewStatus::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "status",
                value: text.to_owned(),
            })
    }
}

impl Serialize for ViewStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ViewStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        std::str::FromStr::from_str(&text).map_err(serde::de::Error::custom)
    }
}

/// `session.view.result`（`sync.schema.json#/$defs/sessionViewResult`）的 body（§11.8）。
///
/// `sessionId` 逐字回显被应答的声明（离页声明因此是 `null`）；`accepted` 必须带未来的
/// `leaseExpiresAt`，`stale` 必须为 `null`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionViewResult {
    pub session_id: Nullable<Uuid>,
    pub view_version: ViewVersion,
    pub status: ViewStatus,
    pub lease_expires_at: Nullable<Timestamp>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSessionViewResult {
    session_id: Nullable<Uuid>,
    view_version: ViewVersion,
    status: ViewStatus,
    lease_expires_at: Nullable<Timestamp>,
}

impl<'de> Deserialize<'de> for SessionViewResult {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawSessionViewResult::deserialize(deserializer)?;
        match raw.status {
            ViewStatus::Accepted if raw.lease_expires_at.is_null() => {
                return Err(serde::de::Error::custom(
                    "status=accepted 时必须给出未来的 leaseExpiresAt",
                ));
            }
            ViewStatus::Stale if !raw.lease_expires_at.is_null() => {
                return Err(serde::de::Error::custom(
                    "status=stale 时 leaseExpiresAt 必须为 null",
                ));
            }
            _ => {}
        }
        Ok(Self {
            session_id: raw.session_id,
            view_version: raw.view_version,
            status: raw.status,
            lease_expires_at: raw.lease_expires_at,
        })
    }
}

/// `preparationAction`（`sync.schema.json#/$defs/preparationAction`）：准备控制面的闭合 action。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreparationAction {
    /// `allocate`
    Allocate,
    /// `restore`
    Restore,
    /// `configure`
    Configure,
    /// `renew`
    Renew,
    /// `release`
    Release,
}

impl PreparationAction {
    pub const ALL: [PreparationAction; 5] = [
        PreparationAction::Allocate,
        PreparationAction::Restore,
        PreparationAction::Configure,
        PreparationAction::Renew,
        PreparationAction::Release,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PreparationAction::Allocate => "allocate",
            PreparationAction::Restore => "restore",
            PreparationAction::Configure => "configure",
            PreparationAction::Renew => "renew",
            PreparationAction::Release => "release",
        }
    }
}

impl std::fmt::Display for PreparationAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PreparationAction {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "action",
                value: text.to_owned(),
            })
    }
}

impl Serialize for PreparationAction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PreparationAction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_str(&text).map_err(DeError::custom)
    }
}

/// `session.preparation.result.body.status`：闭合五态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreparationStatus {
    /// `ready`
    Ready,
    /// `committed`
    Committed,
    /// `stale`
    Stale,
    /// `released`
    Released,
    /// `failed`
    Failed,
}

impl PreparationStatus {
    pub const ALL: [PreparationStatus; 5] = [
        PreparationStatus::Ready,
        PreparationStatus::Committed,
        PreparationStatus::Stale,
        PreparationStatus::Released,
        PreparationStatus::Failed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PreparationStatus::Ready => "ready",
            PreparationStatus::Committed => "committed",
            PreparationStatus::Stale => "stale",
            PreparationStatus::Released => "released",
            PreparationStatus::Failed => "failed",
        }
    }
}

impl std::fmt::Display for PreparationStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PreparationStatus {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "status",
                value: text.to_owned(),
            })
    }
}

impl Serialize for PreparationStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PreparationStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_str(&text).map_err(DeError::custom)
    }
}

/// `session.preparation.result.body.reason`（`status = "stale"`）：资源代失效的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreparationStaleReason {
    /// `runtime_epoch_mismatch`
    RuntimeEpochMismatch,
    /// `generation_mismatch`
    GenerationMismatch,
    /// `released`
    Released,
}

impl PreparationStaleReason {
    pub const ALL: [PreparationStaleReason; 3] = [
        PreparationStaleReason::RuntimeEpochMismatch,
        PreparationStaleReason::GenerationMismatch,
        PreparationStaleReason::Released,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PreparationStaleReason::RuntimeEpochMismatch => "runtime_epoch_mismatch",
            PreparationStaleReason::GenerationMismatch => "generation_mismatch",
            PreparationStaleReason::Released => "released",
        }
    }
}

impl std::fmt::Display for PreparationStaleReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PreparationStaleReason {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "reason",
                value: text.to_owned(),
            })
    }
}

impl Serialize for PreparationStaleReason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PreparationStaleReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_str(&text).map_err(DeError::custom)
    }
}

/// `session.preparation`（`sync.schema.json#/$defs/sessionPreparation`）的 body（§11.9）。
///
/// 临时控制面的闭合 action 载荷：`allocate` 只带 Agent/目录与标签页归属；`restore` 必带凭据，
/// `knownRuntimeEpoch`/`knownGeneration` 只作旧状态提示（不参与拒绝判定）；`configure`/`renew`/
/// `release` 携带凭据与待操作资源代。所有分支都带 `operationId`（连接期有界去重）。构造与反序列化
/// 都按 action 钉死「哪些字段必须出现、哪些禁止出现」——混用字段是形状错误，不是被忽略的多余键。
///
/// `session_id` 只为 `allocate` 之外的分支存在：正式 ID 由 Owner 在 `allocate` 时分配并在
/// `session.preparation.result` 中回传，客户端不得自行指定（design D2/D3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPreparation {
    pub operation_id: Uuid,
    pub action: PreparationAction,
    /// `allocate` 为 `None`；`restore`/`configure`/`renew`/`release` 必须给出。
    pub session_id: Option<Uuid>,
    pub detail: PreparationRequestDetail,
}

/// 准备 action 的闭合字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparationRequestDetail {
    /// `allocate`：Agent、目录来源与标签页归属。
    Allocate {
        agent_id: NonEmptyText<128>,
        workspace: PreparationWorkspaceRef,
        tab_nonce: Base64Url<32>,
    },
    /// `restore`：凭据 + 可选的旧状态提示。
    Restore {
        receipt: PreparationReceipt,
        known_runtime_epoch: Option<Uuid>,
        known_generation: Option<PositiveDecimalString>,
    },
    /// `configure`：在当前资源代上应用模式/配置（至少一个）。
    Configure {
        receipt: PreparationReceipt,
        runtime_epoch: Uuid,
        generation: PositiveDecimalString,
        mode: Option<NonEmptyText<256>>,
        config_options: Option<PreparationConfigSelection>,
    },
    /// `renew`：续租当前资源代。
    Renew {
        receipt: PreparationReceipt,
        runtime_epoch: Uuid,
        generation: PositiveDecimalString,
    },
    /// `release`：释放当前资源代。
    Release {
        receipt: PreparationReceipt,
        runtime_epoch: Uuid,
        generation: PositiveDecimalString,
    },
}

/// allocate 的目录来源：`directoryId` 与 `workspacePath` **恰好一个**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparationWorkspaceRef {
    /// 已登记目录的标识（32 字符小写十六进制）。
    Directory { directory_id: NonEmptyText<32> },
    /// 客户端提交的绝对路径（1..=4096 字符）。
    Path { workspace_path: NonEmptyText<4096> },
}

impl Serialize for PreparationWorkspaceRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            PreparationWorkspaceRef::Directory { directory_id } => {
                map.serialize_entry("directoryId", directory_id)?;
            }
            PreparationWorkspaceRef::Path { workspace_path } => {
                map.serialize_entry("workspacePath", workspace_path)?;
            }
        }
        map.end()
    }
}

impl PreparationWorkspaceRef {
    fn validate(value: &PreparationWorkspaceRef) -> Result<(), ValueError> {
        match value {
            PreparationWorkspaceRef::Directory { directory_id } => {
                let text = directory_id.as_str();
                if text.len() != 32
                    || !text
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                {
                    return Err(ValueError::Pattern {
                        field: "directoryId",
                        value: text.to_owned(),
                    });
                }
            }
            PreparationWorkspaceRef::Path { .. } => {}
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawPreparationWorkspaceRef {
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    directory_id: Option<NonEmptyText<32>>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    workspace_path: Option<NonEmptyText<4096>>,
}

impl<'de> Deserialize<'de> for PreparationWorkspaceRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawPreparationWorkspaceRef::deserialize(deserializer)?;
        let value = match (raw.directory_id, raw.workspace_path) {
            (Some(directory_id), None) => PreparationWorkspaceRef::Directory { directory_id },
            (None, Some(workspace_path)) => PreparationWorkspaceRef::Path { workspace_path },
            _ => {
                return Err(DeError::custom(ValueError::Shape {
                    field: "workspace",
                    expected: "directoryId 与 workspacePath 必须恰好给出一个",
                }));
            }
        };
        Self::validate(&value).map_err(DeError::custom)?;
        Ok(value)
    }
}

impl SessionPreparation {
    pub fn new(
        operation_id: Uuid,
        action: PreparationAction,
        session_id: Option<Uuid>,
        detail: PreparationRequestDetail,
    ) -> Self {
        Self {
            operation_id,
            action,
            session_id,
            detail,
        }
    }
}

impl Serialize for SessionPreparation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("operationId", &self.operation_id)?;
        map.serialize_entry("action", &self.action)?;
        if let Some(session_id) = &self.session_id {
            map.serialize_entry("sessionId", session_id)?;
        }
        match &self.detail {
            PreparationRequestDetail::Allocate {
                agent_id,
                workspace,
                tab_nonce,
            } => {
                map.serialize_entry("agentId", agent_id)?;
                map.serialize_entry("workspace", workspace)?;
                map.serialize_entry("tabNonce", tab_nonce)?;
            }
            PreparationRequestDetail::Restore {
                receipt,
                known_runtime_epoch,
                known_generation,
            } => {
                map.serialize_entry("receipt", receipt)?;
                if let Some(value) = known_runtime_epoch {
                    map.serialize_entry("knownRuntimeEpoch", value)?;
                }
                if let Some(value) = known_generation {
                    map.serialize_entry("knownGeneration", value)?;
                }
            }
            PreparationRequestDetail::Configure {
                receipt,
                runtime_epoch,
                generation,
                mode,
                config_options,
            } => {
                map.serialize_entry("receipt", receipt)?;
                map.serialize_entry("runtimeEpoch", runtime_epoch)?;
                map.serialize_entry("generation", generation)?;
                if let Some(value) = mode {
                    map.serialize_entry("mode", value)?;
                }
                if let Some(value) = config_options {
                    map.serialize_entry("configOptions", value)?;
                }
            }
            PreparationRequestDetail::Renew {
                receipt,
                runtime_epoch,
                generation,
            }
            | PreparationRequestDetail::Release {
                receipt,
                runtime_epoch,
                generation,
            } => {
                map.serialize_entry("receipt", receipt)?;
                map.serialize_entry("runtimeEpoch", runtime_epoch)?;
                map.serialize_entry("generation", generation)?;
            }
        }
        map.end()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSessionPreparation {
    operation_id: Uuid,
    action: PreparationAction,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    session_id: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    agent_id: Option<NonEmptyText<128>>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    workspace: Option<PreparationWorkspaceRef>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    tab_nonce: Option<Base64Url<32>>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    receipt: Option<PreparationReceipt>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    runtime_epoch: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    generation: Option<PositiveDecimalString>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    mode: Option<NonEmptyText<256>>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    config_options: Option<PreparationConfigSelection>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    known_runtime_epoch: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    known_generation: Option<PositiveDecimalString>,
}

fn forbidden(field: &'static str) -> ValueError {
    ValueError::Shape {
        field,
        expected: "该 action 不允许此字段（sessionPreparation 的闭合分支）",
    }
}

fn required(field: &'static str) -> ValueError {
    ValueError::Shape {
        field,
        expected: "该 action 必须携带此字段（sessionPreparation 的闭合分支）",
    }
}

impl<'de> Deserialize<'de> for SessionPreparation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawSessionPreparation::deserialize(deserializer)?;
        let action = raw.action;

        // 除 action 自身字段外，其余任何字段出现都按形状错误拒绝。
        let forbid = |present: bool, field: &'static str| -> Result<(), D::Error> {
            if present {
                Err(DeError::custom(forbidden(field)))
            } else {
                Ok(())
            }
        };
        let detail = match action {
            PreparationAction::Allocate => {
                forbid(raw.session_id.is_some(), "sessionId")?;
                forbid(raw.receipt.is_some(), "receipt")?;
                forbid(raw.runtime_epoch.is_some(), "runtimeEpoch")?;
                forbid(raw.generation.is_some(), "generation")?;
                forbid(raw.mode.is_some(), "mode")?;
                forbid(raw.config_options.is_some(), "configOptions")?;
                forbid(raw.known_runtime_epoch.is_some(), "knownRuntimeEpoch")?;
                forbid(raw.known_generation.is_some(), "knownGeneration")?;
                let agent_id = raw
                    .agent_id
                    .ok_or_else(|| DeError::custom(required("agentId")))?;
                let workspace = raw
                    .workspace
                    .ok_or_else(|| DeError::custom(required("workspace")))?;
                let tab_nonce = raw
                    .tab_nonce
                    .ok_or_else(|| DeError::custom(required("tabNonce")))?;
                PreparationRequestDetail::Allocate {
                    agent_id,
                    workspace,
                    tab_nonce,
                }
            }
            PreparationAction::Restore => {
                forbid(raw.agent_id.is_some(), "agentId")?;
                forbid(raw.workspace.is_some(), "workspace")?;
                forbid(raw.tab_nonce.is_some(), "tabNonce")?;
                forbid(raw.runtime_epoch.is_some(), "runtimeEpoch")?;
                forbid(raw.generation.is_some(), "generation")?;
                forbid(raw.mode.is_some(), "mode")?;
                forbid(raw.config_options.is_some(), "configOptions")?;
                let receipt = raw
                    .receipt
                    .ok_or_else(|| DeError::custom(required("receipt")))?;
                PreparationRequestDetail::Restore {
                    receipt,
                    known_runtime_epoch: raw.known_runtime_epoch,
                    known_generation: raw.known_generation,
                }
            }
            PreparationAction::Configure => {
                forbid(raw.agent_id.is_some(), "agentId")?;
                forbid(raw.workspace.is_some(), "workspace")?;
                forbid(raw.tab_nonce.is_some(), "tabNonce")?;
                forbid(raw.known_runtime_epoch.is_some(), "knownRuntimeEpoch")?;
                forbid(raw.known_generation.is_some(), "knownGeneration")?;
                if raw.mode.is_none() && raw.config_options.is_none() {
                    return Err(DeError::custom(required("mode | configOptions")));
                }
                PreparationRequestDetail::Configure {
                    receipt: raw
                        .receipt
                        .ok_or_else(|| DeError::custom(required("receipt")))?,
                    runtime_epoch: raw
                        .runtime_epoch
                        .ok_or_else(|| DeError::custom(required("runtimeEpoch")))?,
                    generation: raw
                        .generation
                        .ok_or_else(|| DeError::custom(required("generation")))?,
                    mode: raw.mode,
                    config_options: raw.config_options,
                }
            }
            PreparationAction::Renew | PreparationAction::Release => {
                forbid(raw.agent_id.is_some(), "agentId")?;
                forbid(raw.workspace.is_some(), "workspace")?;
                forbid(raw.tab_nonce.is_some(), "tabNonce")?;
                forbid(raw.mode.is_some(), "mode")?;
                forbid(raw.config_options.is_some(), "configOptions")?;
                forbid(raw.known_runtime_epoch.is_some(), "knownRuntimeEpoch")?;
                forbid(raw.known_generation.is_some(), "knownGeneration")?;
                let receipt = raw
                    .receipt
                    .ok_or_else(|| DeError::custom(required("receipt")))?;
                let runtime_epoch = raw
                    .runtime_epoch
                    .ok_or_else(|| DeError::custom(required("runtimeEpoch")))?;
                let generation = raw
                    .generation
                    .ok_or_else(|| DeError::custom(required("generation")))?;
                if action == PreparationAction::Renew {
                    PreparationRequestDetail::Renew {
                        receipt,
                        runtime_epoch,
                        generation,
                    }
                } else {
                    PreparationRequestDetail::Release {
                        receipt,
                        runtime_epoch,
                        generation,
                    }
                }
            }
        };
        // 正式 ID 只对 `allocate` 之外的分支有意义；`allocate` 的 ID 由 Owner 分配并回传。
        let session_id = match action {
            PreparationAction::Allocate => None,
            _ => Some(
                raw.session_id
                    .ok_or_else(|| DeError::custom(required("sessionId")))?,
            ),
        };

        Ok(Self {
            operation_id: raw.operation_id,
            action,
            session_id,
            detail,
        })
    }
}

/// `session.preparation.result`（`sync.schema.json#/$defs/sessionPreparationResult`）的 body（§11.9）。
///
/// 闭合 `status` 判别联合：`ready` 给出凭据/资源代/Agent 确认过的真实设置；`committed` 只给首次
/// 提交关联（**不是** `completed`）；`stale` 只来自 `configure`/`renew`/`release` 与附件 begin/
/// manifest；`released` 无额外字段；`failed` 带结构化错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPreparationResult {
    pub operation_id: Uuid,
    pub action: PreparationAction,
    pub session_id: Uuid,
    pub outcome: PreparationOutcome,
}

/// 准备结果的分支。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // wire 联合本身即分支形状；装箱会改变跨 crate 的解码 API。
pub enum PreparationOutcome {
    /// `ready`
    Ready {
        receipt: PreparationReceipt,
        runtime_epoch: Uuid,
        generation: PositiveDecimalString,
        modes: Vec<ModeRef>,
        config_options: Vec<ConfigOptionView>,
        lease_expires_at: Timestamp,
    },
    /// `committed`
    Committed {
        first_submission_id: Uuid,
        first_request_id: Uuid,
        current_request_id: Uuid,
        first_message_id: Uuid,
    },
    /// `stale`
    Stale { reason: PreparationStaleReason },
    /// `released`
    Released,
    /// `failed`
    Failed { error: PublicError },
}

impl PreparationOutcome {
    pub fn status(&self) -> PreparationStatus {
        match self {
            PreparationOutcome::Ready { .. } => PreparationStatus::Ready,
            PreparationOutcome::Committed { .. } => PreparationStatus::Committed,
            PreparationOutcome::Stale { .. } => PreparationStatus::Stale,
            PreparationOutcome::Released => PreparationStatus::Released,
            PreparationOutcome::Failed { .. } => PreparationStatus::Failed,
        }
    }
}

impl Serialize for SessionPreparationResult {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("operationId", &self.operation_id)?;
        map.serialize_entry("action", &self.action)?;
        map.serialize_entry("sessionId", &self.session_id)?;
        map.serialize_entry("status", &self.outcome.status())?;
        match &self.outcome {
            PreparationOutcome::Ready {
                receipt,
                runtime_epoch,
                generation,
                modes,
                config_options,
                lease_expires_at,
            } => {
                map.serialize_entry("receipt", receipt)?;
                map.serialize_entry("runtimeEpoch", runtime_epoch)?;
                map.serialize_entry("generation", generation)?;
                map.serialize_entry("modes", modes)?;
                map.serialize_entry("configOptions", config_options)?;
                map.serialize_entry("leaseExpiresAt", lease_expires_at)?;
            }
            PreparationOutcome::Committed {
                first_submission_id,
                first_request_id,
                current_request_id,
                first_message_id,
            } => {
                map.serialize_entry("firstSubmissionId", first_submission_id)?;
                map.serialize_entry("firstRequestId", first_request_id)?;
                map.serialize_entry("currentRequestId", current_request_id)?;
                map.serialize_entry("firstMessageId", first_message_id)?;
            }
            PreparationOutcome::Stale { reason } => {
                map.serialize_entry("reason", reason)?;
            }
            PreparationOutcome::Released => {}
            PreparationOutcome::Failed { error } => {
                map.serialize_entry("error", error)?;
            }
        }
        map.end()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSessionPreparationResult {
    operation_id: Uuid,
    action: PreparationAction,
    session_id: Uuid,
    status: PreparationStatus,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    receipt: Option<PreparationReceipt>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    runtime_epoch: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    generation: Option<PositiveDecimalString>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    modes: Option<Vec<ModeRef>>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    config_options: Option<Vec<ConfigOptionView>>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    lease_expires_at: Option<Timestamp>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    first_submission_id: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    first_request_id: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    current_request_id: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    first_message_id: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    reason: Option<PreparationStaleReason>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    error: Option<PublicError>,
}

impl<'de> Deserialize<'de> for SessionPreparationResult {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawSessionPreparationResult::deserialize(deserializer)?;
        // 每个 status 只接受自己的字段；其余字段出现即形状错误。
        let forbid = |present: bool, field: &'static str| -> Result<(), D::Error> {
            if present {
                Err(DeError::custom(forbidden(field)))
            } else {
                Ok(())
            }
        };
        let outcome = match raw.status {
            PreparationStatus::Ready => {
                forbid(raw.first_submission_id.is_some(), "firstSubmissionId")?;
                forbid(raw.first_request_id.is_some(), "firstRequestId")?;
                forbid(raw.current_request_id.is_some(), "currentRequestId")?;
                forbid(raw.first_message_id.is_some(), "firstMessageId")?;
                forbid(raw.reason.is_some(), "reason")?;
                forbid(raw.error.is_some(), "error")?;
                PreparationOutcome::Ready {
                    receipt: raw
                        .receipt
                        .ok_or_else(|| DeError::custom(required("receipt")))?,
                    runtime_epoch: raw
                        .runtime_epoch
                        .ok_or_else(|| DeError::custom(required("runtimeEpoch")))?,
                    generation: raw
                        .generation
                        .ok_or_else(|| DeError::custom(required("generation")))?,
                    modes: raw
                        .modes
                        .ok_or_else(|| DeError::custom(required("modes")))?,
                    config_options: raw
                        .config_options
                        .ok_or_else(|| DeError::custom(required("configOptions")))?,
                    lease_expires_at: raw
                        .lease_expires_at
                        .ok_or_else(|| DeError::custom(required("leaseExpiresAt")))?,
                }
            }
            PreparationStatus::Committed => {
                forbid(raw.receipt.is_some(), "receipt")?;
                forbid(raw.runtime_epoch.is_some(), "runtimeEpoch")?;
                forbid(raw.generation.is_some(), "generation")?;
                forbid(raw.modes.is_some(), "modes")?;
                forbid(raw.config_options.is_some(), "configOptions")?;
                forbid(raw.lease_expires_at.is_some(), "leaseExpiresAt")?;
                forbid(raw.reason.is_some(), "reason")?;
                forbid(raw.error.is_some(), "error")?;
                PreparationOutcome::Committed {
                    first_submission_id: raw
                        .first_submission_id
                        .ok_or_else(|| DeError::custom(required("firstSubmissionId")))?,
                    first_request_id: raw
                        .first_request_id
                        .ok_or_else(|| DeError::custom(required("firstRequestId")))?,
                    current_request_id: raw
                        .current_request_id
                        .ok_or_else(|| DeError::custom(required("currentRequestId")))?,
                    first_message_id: raw
                        .first_message_id
                        .ok_or_else(|| DeError::custom(required("firstMessageId")))?,
                }
            }
            PreparationStatus::Stale => {
                forbid(raw.receipt.is_some(), "receipt")?;
                forbid(raw.runtime_epoch.is_some(), "runtimeEpoch")?;
                forbid(raw.generation.is_some(), "generation")?;
                forbid(raw.modes.is_some(), "modes")?;
                forbid(raw.config_options.is_some(), "configOptions")?;
                forbid(raw.lease_expires_at.is_some(), "leaseExpiresAt")?;
                forbid(raw.first_submission_id.is_some(), "firstSubmissionId")?;
                forbid(raw.first_request_id.is_some(), "firstRequestId")?;
                forbid(raw.current_request_id.is_some(), "currentRequestId")?;
                forbid(raw.first_message_id.is_some(), "firstMessageId")?;
                forbid(raw.error.is_some(), "error")?;
                PreparationOutcome::Stale {
                    reason: raw
                        .reason
                        .ok_or_else(|| DeError::custom(required("reason")))?,
                }
            }
            PreparationStatus::Released => {
                for (present, field) in [
                    (raw.receipt.is_some(), "receipt"),
                    (raw.runtime_epoch.is_some(), "runtimeEpoch"),
                    (raw.generation.is_some(), "generation"),
                    (raw.modes.is_some(), "modes"),
                    (raw.config_options.is_some(), "configOptions"),
                    (raw.lease_expires_at.is_some(), "leaseExpiresAt"),
                    (raw.first_submission_id.is_some(), "firstSubmissionId"),
                    (raw.first_request_id.is_some(), "firstRequestId"),
                    (raw.current_request_id.is_some(), "currentRequestId"),
                    (raw.first_message_id.is_some(), "firstMessageId"),
                    (raw.reason.is_some(), "reason"),
                    (raw.error.is_some(), "error"),
                ] {
                    forbid(present, field)?;
                }
                PreparationOutcome::Released
            }
            PreparationStatus::Failed => {
                for (present, field) in [
                    (raw.receipt.is_some(), "receipt"),
                    (raw.runtime_epoch.is_some(), "runtimeEpoch"),
                    (raw.generation.is_some(), "generation"),
                    (raw.modes.is_some(), "modes"),
                    (raw.config_options.is_some(), "configOptions"),
                    (raw.lease_expires_at.is_some(), "leaseExpiresAt"),
                    (raw.first_submission_id.is_some(), "firstSubmissionId"),
                    (raw.first_request_id.is_some(), "firstRequestId"),
                    (raw.current_request_id.is_some(), "currentRequestId"),
                    (raw.first_message_id.is_some(), "firstMessageId"),
                    (raw.reason.is_some(), "reason"),
                ] {
                    forbid(present, field)?;
                }
                PreparationOutcome::Failed {
                    error: raw
                        .error
                        .ok_or_else(|| DeError::custom(required("error")))?,
                }
            }
        };

        Ok(Self {
            operation_id: raw.operation_id,
            action: raw.action,
            session_id: raw.session_id,
            outcome,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAPSHOT: &str = "8194de43-e213-423d-acf4-2e3549304566";
    const EPOCH: &str = "00384a03-bc90-4095-b65d-82fb8cc47e13";

    fn chunk(resource: &str) -> Result<SnapshotChunk, serde_json::Error> {
        serde_json::from_str(&format!(
            r#"{{"snapshotId":"{SNAPSHOT}","chunkIndex":"0","resource":"{resource}","items":[]}}"#
        ))
    }

    #[test]
    fn snapshot_chunk_carries_only_catalog_resources() {
        assert_eq!(SnapshotResource::ALL.len(), 3, "快照只承载三类清单资源");
        for resource in ["sessions", "workspaces", "agents"] {
            assert!(chunk(resource).is_ok(), "清单类资源必须可下发：{resource}");
        }
        // 六类会话明细资源不进快照，对本地会话同样如此（§9.4 / design D1）。
        for resource in [
            "messages",
            "turns",
            "pending_interactions",
            "config_options",
            "capabilities",
            "history",
        ] {
            assert!(
                chunk(resource).is_err(),
                "会话明细资源不得进入快照：{resource}"
            );
        }
        assert!(chunk("everything").is_err(), "未登记的 resource 必须被拒");
    }

    #[test]
    fn chunk_count_matches_the_resources_a_snapshot_can_carry() {
        let begin = |count: u32| {
            serde_json::from_str::<SnapshotBegin>(&format!(
                r#"{{"snapshotId":"{SNAPSHOT}","cursor":{{"serverEpoch":"{EPOCH}","globalSequence":"2318"}},"schemaVersion":1,"chunkCount":{count}}}"#
            ))
        };
        for count in [1, 3] {
            assert!(begin(count).is_ok(), "chunkCount {count} 必须被接受");
        }
        // 0 与超出资源种类数的取值都意味着「声明了发不出的块」，与 §9.4 的 chunkCount 口径冲突。
        for count in [0, 4, 100_000] {
            assert!(begin(count).is_err(), "chunkCount {count} 必须被拒");
        }
    }
}
