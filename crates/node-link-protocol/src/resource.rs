//! `resource.*` 家族 body（`schemas/node-link/v1/resource.schema.json`）：attachment 生命周期、游标订阅、
//! 快照重建、origin 事件投递与累计 ACK。
//!
//! 对应 `docs/NODE_LINK_PROTOCOL.md` §12.4。这里只做 wire 形状与取值域校验；**会话层**的状态判定不在这里：
//! `attachmentGeneration` 是否是当前 generation、`sessionRef` 是否属于本连接的 attachment、
//! `cursor.originEpoch` 与会话 origin epoch 是否一致、`originEventId` 去重、`chunkIndex` 是否连续、
//! `chunkCount` 在 `snapshot_begin`/`snapshot_end` 之间是否一致、`snapshotDigest`/`payloadDigest` 是否
//! 与正文匹配、`resource.ack.cursor` 是否单调不减——都由持有 attachment 的会话层判定。
//!
//! 本模块内**可判定**的跨字段一致性有两条，都在反序列化时校验：
//!
//! 1. `resource.snapshot_chunk`：`items` 的元素形状由同一 body 的 `resource` 决定（schema 的
//!    `allOf`/`if`/`then`），不符报 [`ValueError::Shape`]；`session_meta` 的 `items` 最多 1 项、其余
//!    最多 500 项，超限报 [`ValueError::TooManyItems`]。
//! 2. `resource.event`：`payload` 里 `view` 与 `acp` 至少要出现一个（schema `payload.anyOf`），
//!    都为缺失报 [`ValueError::Shape`]。
//!
//! `payload.view` 是唯一允许携带未登记字段的位置（开放扩展点），按 [`RawObject`] 原样保留字节；
//! `payload.acp` 是 `common.schema.json#/$defs/rawAcp` 的两个互斥形状（[`RawAcp`]）。

use std::fmt;
use std::str::FromStr;

use serde::de::{DeserializeOwned, Error as DeError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use acpr_wire::{
    Base64Url, BoundedU64, DecimalString, Nullable, ProtocolVersionV1, RawAcp, RawObject,
    Timestamp, Uuid, ValueError,
};

use crate::common::{OriginCursor, PendingInteraction, RemoteSessionRef, SessionMeta};

/// `resource.event.body.eventType`：`^[a-z0-9_.-]{1,128}$`。
///
/// 字符集合法但未登记的取值仍是合法事件类型——`docs/NODE_LINK_PROTOCOL.md` §11.4 把事件类型定义为
/// 与 Sync 共享的开放集合，未知类型必须按未知事件降级处理。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventType(String);

impl EventType {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        let length = text.chars().count();
        let charset_ok = text.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-')
        });
        if length == 0 || length > 128 || !charset_ok {
            return Err(ValueError::Enumerated {
                field: "eventType",
                value: text.to_owned(),
            });
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for EventType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for EventType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(DeError::custom)
    }
}

/// `resource.detach.body.reason` 的取值（`resource.schema.json#/$defs/resourceDetach` 的 `enum`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DetachReason {
    ClientRequest,
    ExportRevoked,
    OwnerUnavailable,
}

impl DetachReason {
    pub const ALL: [DetachReason; 3] = [
        DetachReason::ClientRequest,
        DetachReason::ExportRevoked,
        DetachReason::OwnerUnavailable,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DetachReason::ClientRequest => "client_request",
            DetachReason::ExportRevoked => "export_revoked",
            DetachReason::OwnerUnavailable => "owner_unavailable",
        }
    }
}

impl fmt::Display for DetachReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for DetachReason {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        DetachReason::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
            .ok_or_else(|| ValueError::Enumerated {
                field: "reason",
                value: text.to_owned(),
            })
    }
}

impl Serialize for DetachReason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DetachReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        DetachReason::from_str(&text).map_err(DeError::custom)
    }
}

/// `resource.snapshot_chunk.body.resource`：同一 body 里的 `items` 元素形状由它决定
/// （`resource.schema.json#/$defs/resourceSnapshotChunk` 的 `allOf`/`if`/`then`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapshotResource {
    SessionMeta,
    PendingInteractions,
}

impl SnapshotResource {
    pub const ALL: [SnapshotResource; 2] = [
        SnapshotResource::SessionMeta,
        SnapshotResource::PendingInteractions,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotResource::SessionMeta => "session_meta",
            SnapshotResource::PendingInteractions => "pending_interactions",
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

/// `resource.snapshot_chunk` 中 `resource == "session_meta"` 的单个元素
/// （`resource.schema.json#/$defs/snapshotItem.session_meta`）。
///
/// 只承载会话元数据，不含任何会话正文（正文只能按 origin cursor 经 [`Event`] 重放，
/// `docs/NODE_LINK_PROTOCOL.md` §12.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotItemSessionMeta {
    #[serde(rename = "sessionRef")]
    pub session_ref: RemoteSessionRef,
    #[serde(rename = "sessionMeta")]
    pub session_meta: SessionMeta,
}

/// `resource.snapshot_chunk.items`：元素类型由同一 body 的 `resource` 决定
/// （`resource.schema.json#/$defs/resourceSnapshotChunk` 的 `allOf`/`if`/`then`）。
///
/// 枚举变体与 [`SnapshotResource`] 一一对应，因此「variant 与 `resource` 不一致」的值不存在：
/// 反序列化由 [`SnapshotItems::parse`] 按 `resource` 分派，序列化只写数组本身。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotItems {
    SessionMeta(Vec<SnapshotItemSessionMeta>),
    PendingInteractions(Vec<PendingInteraction>),
}

impl SnapshotItems {
    /// `resource.snapshot_chunk.body.items` 的最大长度（schema 的 `maxItems`）。
    pub const MAX_ITEMS: usize = 500;

    /// `resource == "session_meta"` 时 `items` 的最大长度（schema 的 `then` 分支）。
    pub const SESSION_META_MAX_ITEMS: usize = 1;

    /// 按 `resource` 解析 `items` 数组。
    ///
    /// 元素形状与 `resource` 不符时返回 [`ValueError::Shape`]（该位置只允许对应
    /// `snapshotItem.*` 定义的元素），超过该分支的 `maxItems` 时返回 [`ValueError::TooManyItems`]。
    pub fn parse(resource: SnapshotResource, raw: &RawValue) -> Result<Self, ValueError> {
        Ok(match resource {
            SnapshotResource::SessionMeta => {
                SnapshotItems::SessionMeta(parse_items::<SnapshotItemSessionMeta>(
                    raw,
                    "snapshotItem.session_meta",
                    Self::SESSION_META_MAX_ITEMS,
                )?)
            }
            SnapshotResource::PendingInteractions => {
                SnapshotItems::PendingInteractions(parse_items::<PendingInteraction>(
                    raw,
                    "snapshotItem.pending_interactions",
                    Self::MAX_ITEMS,
                )?)
            }
        })
    }

    /// 本组 `items` 对应的 `resource` 取值。
    pub fn resource(&self) -> SnapshotResource {
        match self {
            SnapshotItems::SessionMeta(_) => SnapshotResource::SessionMeta,
            SnapshotItems::PendingInteractions(_) => SnapshotResource::PendingInteractions,
        }
    }

    /// 元素个数。
    pub fn len(&self) -> usize {
        match self {
            SnapshotItems::SessionMeta(items) => items.len(),
            SnapshotItems::PendingInteractions(items) => items.len(),
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
            SnapshotItems::SessionMeta(items) => items.serialize(serializer),
            SnapshotItems::PendingInteractions(items) => items.serialize(serializer),
        }
    }
}

/// 按 `expected` 指的 `snapshotItem.*` 定义解析 `items` 数组，并施加该分支的 `maxItems`。
fn parse_items<T: DeserializeOwned>(
    raw: &RawValue,
    expected: &'static str,
    max: usize,
) -> Result<Vec<T>, ValueError> {
    let items: Vec<T> = serde_json::from_str(raw.get()).map_err(|_| ValueError::Shape {
        field: "items",
        expected,
    })?;
    if items.len() > max {
        return Err(ValueError::TooManyItems {
            field: "items",
            max,
            actual: items.len(),
        });
    }
    Ok(items)
}

/// `resource.attach` 的 body（`resource.schema.json#/$defs/resourceAttach`）。
///
/// 为耐久会话身份申请新的临时路由凭据：同一个 `remoteSessionRef` 可以反复 attach，每次 attach 得到新的
/// `attachmentId`/`attachmentGeneration`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attach {
    #[serde(rename = "remoteSessionRef")]
    pub remote_session_ref: RemoteSessionRef,
}

/// `resource.attached` 的 body（`resource.schema.json#/$defs/resourceAttached`）。
///
/// 三个字段都必需：新 generation 生效后，旧 generation 的 frame 一律拒绝（`docs/NODE_LINK_PROTOCOL.md`
/// §12.4；拒绝逻辑在会话层）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attached {
    #[serde(rename = "attachmentId")]
    pub attachment_id: Uuid,
    #[serde(rename = "attachmentGeneration")]
    pub attachment_generation: DecimalString,
    #[serde(rename = "sessionMeta")]
    pub session_meta: SessionMeta,
}

/// `resource.detach` 的 body（`resource.schema.json#/$defs/resourceDetach`）。
///
/// `post_mvp`：主动释放 attachment（`docs/NODE_LINK_PROTOCOL.md` §12.4 标注为后续阶段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detach {
    #[serde(rename = "attachmentId")]
    pub attachment_id: Uuid,
    pub reason: DetachReason,
}

/// `resource.subscribe` 的 body（`resource.schema.json#/$defs/resourceSubscribe`）。
///
/// `cursor` 是 required 且可 `null`：`null` 表示先走 snapshot，非空表示从该 origin cursor 增量重放。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subscribe {
    #[serde(rename = "attachmentId")]
    pub attachment_id: Uuid,
    #[serde(rename = "attachmentGeneration")]
    pub attachment_generation: DecimalString,
    pub cursor: Nullable<OriginCursor>,
}

/// `resource.snapshot_begin` 的 body（`resource.schema.json#/$defs/resourceSnapshotBegin`）。
///
/// `cursor` 是 snapshot 的结束点；`schemaVersion` 是 `const 1`，用 [`ProtocolVersionV1`] 表达；
/// `chunkCount` 是 `0..=100000` 的整数，必须与 [`SnapshotEnd::chunk_count`] 一致（一致性由会话层校验）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotBegin {
    #[serde(rename = "snapshotId")]
    pub snapshot_id: Uuid,
    pub cursor: OriginCursor,
    #[serde(rename = "schemaVersion")]
    pub schema_version: ProtocolVersionV1,
    #[serde(rename = "chunkCount")]
    pub chunk_count: BoundedU64<0, 100_000>,
}

/// `resource.snapshot_chunk` 的 body（`resource.schema.json#/$defs/resourceSnapshotChunk`）。
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

    /// 本 chunk 的元素，最多该 `resource` 分支允许的项数。
    pub fn items(&self) -> &SnapshotItems {
        &self.items
    }
}

/// `resource.snapshot_chunk` 的 wire 形状：`items` 先按原始字节收下，再由 `resource` 决定如何解析。
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

/// `resource.snapshot_end` 的 body（`resource.schema.json#/$defs/resourceSnapshotEnd`）。
///
/// `snapshotDigest` 是对每个完整 `resource.snapshot_chunk` 消息的原始 UTF-8 bytes 分别求 SHA-256、
/// 按 `chunkIndex` 顺序连接后再求一次 SHA-256（`docs/NODE_LINK_PROTOCOL.md` §12.4，与
/// `SYNC_PROTOCOL.md` §9.4 相同），wire 上是 `base64url32`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotEnd {
    #[serde(rename = "snapshotId")]
    pub snapshot_id: Uuid,
    pub cursor: OriginCursor,
    #[serde(rename = "chunkCount")]
    pub chunk_count: BoundedU64<0, 100_000>,
    #[serde(rename = "snapshotDigest")]
    pub snapshot_digest: Base64Url<32>,
}

/// `resource.event.body.payload`（`resource.schema.json#/$defs/resourceEvent` 的 `payload`）。
///
/// 形状是关闭的（`additionalProperties: false`），`anyOf` 要求 `view` 与 `acp` 至少出现一个：
/// 事件类型有登记 view 时必须携带 `view`；无法形成 view 时必须保留 `acp` 原文档，不得丢弃
/// （§11.4 的共享 view 合同、§12.4）。
///
/// 字段私有 + [`EventPayload::new`] 校验，所以「两个都没有」这种 schema 不允许的值不存在。
/// 反序列化一律经 [`RawEventPayload`] 再 [`TryFrom`]（`try_from` 是 serde 为跨字段校验提供的机制），
/// 那里用 `acpr_wire::deserialize_optional_non_null` 表达「键可缺失、出现时不可为 `null`」——
/// `view` 是对象、`acp` 是对象联合，两者都没有 `null` 分支，因此显式 `null` 被拒绝。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawEventPayload")]
pub struct EventPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    view: Option<RawObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    acp: Option<RawAcp>,
}

impl EventPayload {
    /// 组装 payload；两者都为 `None` 时返回 [`ValueError::Shape`]。
    pub fn new(view: Option<RawObject>, acp: Option<RawAcp>) -> Result<Self, ValueError> {
        if view.is_none() && acp.is_none() {
            return Err(ValueError::Shape {
                field: "payload",
                expected: "payload 至少携带 view 或 acp 之一（NODE_LINK_PROTOCOL §12.4）",
            });
        }
        Ok(Self { view, acp })
    }

    /// 开放扩展点：事件视图，原样保留字节（键序、空白、超出 u64 的整数字面量都不改写）。
    pub fn view(&self) -> Option<&RawObject> {
        self.view.as_ref()
    }

    /// ACP 原文或其不可用说明（`common.schema.json#/$defs/rawAcp` 的两个分支）。
    pub fn acp(&self) -> Option<&RawAcp> {
        self.acp.as_ref()
    }
}

/// [`EventPayload`] 的线形状：关闭形状在这里落地，两个字段都可缺失但不可为 `null`。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEventPayload {
    #[serde(default, deserialize_with = "acpr_wire::deserialize_optional_non_null")]
    view: Option<RawObject>,
    #[serde(default, deserialize_with = "acpr_wire::deserialize_optional_non_null")]
    acp: Option<RawAcp>,
}

impl TryFrom<RawEventPayload> for EventPayload {
    type Error = ValueError;

    fn try_from(raw: RawEventPayload) -> Result<Self, Self::Error> {
        Self::new(raw.view, raw.acp)
    }
}

/// `resource.event` 的 body（`resource.schema.json#/$defs/resourceEvent`）。
///
/// 完整 origin 三元组（`originEventId` + `originEpoch` + `originSequence`）全部必需：缺失任一字段即
/// `nodelink.protocol.schema_invalid`，Access 不得用本地生成的 ID 顶替，也不得把事件降级成本地事件
/// （`docs/NODE_LINK_PROTOCOL.md` §12.4）。`sessionRef` 在本 crate 的 v1 内修订中由可选改为必需
/// （幂等去重与 ACK 都必须按会话归属）；它必须指向当前连接上 attachment 所属的会话，跨会话判定由
/// 会话层完成。`payloadDigest` 是对 `payload` 应用 ACPR-CJ1 后的 SHA-256（§12.4），本模块只校验它是
/// 32 字节的规范 base64url。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    #[serde(rename = "originEventId")]
    pub origin_event_id: Uuid,
    #[serde(rename = "originEpoch")]
    pub origin_epoch: Uuid,
    #[serde(rename = "originSequence")]
    pub origin_sequence: DecimalString,
    #[serde(rename = "sessionRef")]
    pub session_ref: RemoteSessionRef,
    #[serde(rename = "eventType")]
    pub event_type: EventType,
    #[serde(rename = "payloadDigest")]
    pub payload_digest: Base64Url<32>,
    #[serde(rename = "createdAt")]
    pub created_at: Timestamp,
    pub payload: EventPayload,
}

/// `resource.ack` 的 body（`resource.schema.json#/$defs/resourceAck`）。
///
/// 该会话的累计 ACK：此 cursor 及之前的 origin 事件在 Access 侧已进入可恢复的无正文索引。`cursor` 必须
/// 单调不减、`cursor.originEpoch` 必须与该会话的 origin epoch 一致，回退或不符视为
/// `nodelink.protocol.sequence_invalid`（`docs/NODE_LINK_PROTOCOL.md` §12.4/§15；单调性由会话层判定，
/// 本模块只校验形状）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ack {
    #[serde(rename = "sessionRef")]
    pub session_ref: RemoteSessionRef,
    pub cursor: OriginCursor,
}
