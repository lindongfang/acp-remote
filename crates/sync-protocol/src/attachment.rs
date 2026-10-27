//! 独立已认证附件连接（`docs/SYNC_PROTOCOL.md` §3.4）的帧编解码。
//!
//! 附件正文不进入业务 `message.schema.json` 的联合入口：协商 `core.attachment-transfer.v1` 后，客户端
//! 与 Host 另建一条 `/sync/v1/attachments` WSS 连接，本模块只承载那条连接上的 13 个 `attachment.*` 帧。
//! 帧与业务消息共享同一条信封规则（`protocolVersion`、`messageId`、`connectionId`、
//! `connectionSequence`），但 `type` 词表、body 形状与预算都是附件族独有的。
//!
//! 三条边界：
//!
//! - **形状即合同**：每个 body 都是 `additionalProperties: false`（`deny_unknown_fields`），偏移与长度
//!   一律无前导零十进制字符串（[`DecimalString`]），单块原始数据最多 [`CHUNK_SIZE`] 字节；
//! - **错误分类可区分**：语法错误、信封/字段形状错误、未知 `type` 与版本不支持分别映射到同步线的
//!   `protocol.*` 错误码（[`DecodeError::code`]），调用方据此决定消息级拒绝还是关闭连接；
//! - **不解释业务**：本模块不判定传输状态、不计算摘要、不检查会话/主体归属——那些属于服务端适配器与
//!   core 的端口。这里只保证「线上能表示的帧」与「本地类型」一一对应且可逐字节往返。

use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{
    Base64Url, BoundedU64, DecimalString, NonEmptyText, Nullable, ProtocolVersionV1, RawObject,
    Text, Timestamp, Uuid, ValueError,
};
use crate::error::ErrorCode;

/// 单块原始数据的固定上限（§3.4「附件传输预算」：32 KiB，不可协商、不得被 `limits` 下调）。
pub const CHUNK_SIZE: usize = 32768;

/// 单块 `data` 的 base64url 字符上限（`ceil(32768 * 4 / 3)`，与 schema 的 `maxLength` 一致）。
pub const MAX_DATA_CHARS: usize = 43691;

/// 每传输同时最多未确认的块数（§3.4：1，即停等）。
pub const MAX_UNACKNOWLEDGED_CHUNKS: usize = 1;

/// 单设备（本面即单条已认证设备连接）的活动传输上限。
pub const MAX_ACTIVE_TRANSFERS_PER_DEVICE: usize = 2;

/// 单节点的活动传输上限（同一 Host 的所有设备连接合计）。
pub const MAX_ACTIVE_TRANSFERS_PER_NODE: usize = 8;

/// 帧 `chunkSize` 的固定常量：`const 32768`。
pub type FixedChunkSize = BoundedU64<{ CHUNK_SIZE as u64 }, { CHUNK_SIZE as u64 }>;

/// v1 的序号上界（`SYNC_PROTOCOL.md` §3.3）：超出即 `protocol.schema_invalid`，不截断、不回绕。
const MAX_SEQUENCE: u64 = i64::MAX as u64;

/// 清单每页最多登记项数（§3.4：「每页最多 64 个 `transferId`」）。
pub const MANIFEST_PAGE_LIMIT: usize = 64;

/// 13 个 `attachment.*` 帧类型（顺序即 schema 的 `frameType.enum` 与 §3.4 的帧表）。
///
/// 唯一机器定义是 `compatibility/commands/v1/commands.json` 的 `transportFrames`；Rust 常量与
/// schema 的一致性由 `tests/attachment_frames.rs` 逐项断言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FrameType {
    /// `attachment.begin`
    Begin,
    /// `attachment.chunk`
    Chunk,
    /// `attachment.chunk_ack`
    ChunkAck,
    /// `attachment.seal`
    Seal,
    /// `attachment.sealed`
    Sealed,
    /// `attachment.status`
    Status,
    /// `attachment.abort`
    Abort,
    /// `attachment.read`
    Read,
    /// `attachment.read_chunk`
    ReadChunk,
    /// `attachment.manifest_begin`
    ManifestBegin,
    /// `attachment.manifest_page`
    ManifestPage,
    /// `attachment.manifest_sealed`
    ManifestSealed,
    /// `attachment.error`
    Error,
}

impl FrameType {
    /// schema `frameType.enum` 的完整取值，顺序一致。
    pub const ALL: [FrameType; 13] = [
        FrameType::Begin,
        FrameType::Chunk,
        FrameType::ChunkAck,
        FrameType::Seal,
        FrameType::Sealed,
        FrameType::Status,
        FrameType::Abort,
        FrameType::Read,
        FrameType::ReadChunk,
        FrameType::ManifestBegin,
        FrameType::ManifestPage,
        FrameType::ManifestSealed,
        FrameType::Error,
    ];

    /// wire 上的 `type` 文本。
    pub fn as_str(self) -> &'static str {
        match self {
            FrameType::Begin => "attachment.begin",
            FrameType::Chunk => "attachment.chunk",
            FrameType::ChunkAck => "attachment.chunk_ack",
            FrameType::Seal => "attachment.seal",
            FrameType::Sealed => "attachment.sealed",
            FrameType::Status => "attachment.status",
            FrameType::Abort => "attachment.abort",
            FrameType::Read => "attachment.read",
            FrameType::ReadChunk => "attachment.read_chunk",
            FrameType::ManifestBegin => "attachment.manifest_begin",
            FrameType::ManifestPage => "attachment.manifest_page",
            FrameType::ManifestSealed => "attachment.manifest_sealed",
            FrameType::Error => "attachment.error",
        }
    }
}

impl fmt::Display for FrameType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for FrameType {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|frame| frame.as_str() == text)
            .ok_or(())
    }
}

/// 解码失败的四种分类（`conn::wire` 把这些折成同步线的错误码与关闭处置）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// 不是合法 JSON（`protocol.invalid_json`）。
    InvalidJson,
    /// 信封或 body 形状不符合 schema（`protocol.schema_invalid`）。
    Schema,
    /// `type` 不在附件帧词表里（`protocol.type_unsupported`）。
    TypeUnsupported,
    /// `protocolVersion` 不是 1（`protocol.version_unsupported`）。
    VersionUnsupported,
}

impl DecodeError {
    /// 对端可见的错误码。
    pub fn code(self) -> ErrorCode {
        match self {
            DecodeError::InvalidJson => ErrorCode::ProtocolInvalidJson,
            DecodeError::Schema => ErrorCode::ProtocolSchemaInvalid,
            DecodeError::TypeUnsupported => ErrorCode::ProtocolTypeUnsupported,
            DecodeError::VersionUnsupported => ErrorCode::ProtocolVersionUnsupported,
        }
    }
}

/// 传输主体（Sync 面只接受设备；`kind: "node"` 属于跨主体冒用，解码期即拒绝）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSubject {
    /// 恒为 `"device"`。
    #[serde(rename = "kind")]
    pub kind: DeviceSubjectKind,
    /// 已认证设备标识。
    pub id: Uuid,
}

/// `subject.kind` 的唯一合法取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceSubjectKind;

impl DeviceSubjectKind {
    /// wire 取值。
    pub const VALUE: &'static str = "device";
}

impl Serialize for DeviceSubjectKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(Self::VALUE)
    }
}

impl<'de> Deserialize<'de> for DeviceSubjectKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text != Self::VALUE {
            // 换成 `node` 不是「另一种合法主体」，而是另一条协议线的身份：显式拒绝。
            return Err(DeError::custom(ValueError::Enumerated {
                field: "subject.kind",
                value: text,
            }));
        }
        Ok(Self)
    }
}

impl DeviceSubject {
    /// 构造（`id` 必须是已认证设备）。
    pub fn new(id: Uuid) -> Self {
        Self {
            kind: DeviceSubjectKind,
            id,
        }
    }
}

/// 一块原始数据的 base64url 编码（1..=43691 字符 → 1..=32768 字节）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkData(Vec<u8>);

impl ChunkData {
    /// 从原始字节构造。
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ValueError> {
        if bytes.is_empty() {
            return Err(ValueError::TextLength {
                min: 1,
                max: CHUNK_SIZE,
                actual: 0,
            });
        }
        if bytes.len() > CHUNK_SIZE {
            return Err(ValueError::TextLength {
                min: 1,
                max: CHUNK_SIZE,
                actual: bytes.len(),
            });
        }
        Ok(Self(bytes))
    }

    /// 原始字节。
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Serialize for ChunkData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&acpr_transcript::encode_base64url(&self.0))
    }
}

impl<'de> Deserialize<'de> for ChunkData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.len() > MAX_DATA_CHARS {
            return Err(DeError::custom(ValueError::TextLength {
                min: 1,
                max: MAX_DATA_CHARS,
                actual: text.len(),
            }));
        }
        let bytes = acpr_transcript::decode_base64url(&text)
            .map_err(|_| DeError::custom(ValueError::Base64Url(text.clone())))?;
        Self::from_bytes(bytes).map_err(DeError::custom)
    }
}

/// `attachment.begin` 的 body：上传只绑定**已存在的正式会话**（顶层 `sessionId`）。
///
/// 形状闭合（`deny_unknown_fields`）：准备附件目标 `target` 及其运行期字段已被整体移除
/// （`docs/SYNC_PROTOCOL.md` §3.4），出现即解码失败，不存在「被当未来字段忽略」的路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeginBody {
    #[serde(rename = "transferId")]
    pub transfer_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub subject: DeviceSubject,
    #[serde(rename = "totalLength")]
    pub total_length: DecimalString,
    #[serde(rename = "chunkSize")]
    pub chunk_size: FixedChunkSize,
    #[serde(rename = "mediaType")]
    pub media_type: NonEmptyText<255>,
    #[serde(rename = "fileName")]
    pub file_name: NonEmptyText<255>,
}

/// `attachment.chunk` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkBody {
    #[serde(rename = "transferId")]
    pub transfer_id: Uuid,
    pub offset: DecimalString,
    pub data: ChunkData,
}

/// `attachment.chunk_ack` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkAckBody {
    #[serde(rename = "transferId")]
    pub transfer_id: Uuid,
    pub offset: DecimalString,
    pub length: DecimalString,
    pub duplicate: bool,
    #[serde(rename = "nextOffset")]
    pub next_offset: DecimalString,
}

/// `attachment.seal` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SealBody {
    #[serde(rename = "transferId")]
    pub transfer_id: Uuid,
    pub length: DecimalString,
    pub sha256: Base64Url<32>,
}

/// `attachment.sealed` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SealedBody {
    #[serde(rename = "transferId")]
    pub transfer_id: Uuid,
    #[serde(rename = "attachmentId")]
    pub attachment_id: Uuid,
    pub length: DecimalString,
    pub sha256: Base64Url<32>,
    #[serde(rename = "sealedAt")]
    pub sealed_at: Timestamp,
}

/// `attachment.status.state` 的封闭词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    /// 仍在接收（或仍可读取）。
    Receiving,
    /// 已封存。
    Sealed,
    /// 已被取消。
    Aborted,
    /// 闲置超时后被清理。
    Expired,
}

/// `attachment.status` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusBody {
    #[serde(rename = "transferId")]
    pub transfer_id: Uuid,
    pub state: TransferState,
    #[serde(rename = "receivedLength")]
    pub received_length: DecimalString,
    #[serde(rename = "nextOffset")]
    pub next_offset: DecimalString,
    #[serde(rename = "sealedSha256")]
    pub sealed_sha256: Nullable<Base64Url<32>>,
}

/// `attachment.abort.reason` 的封闭词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbortReason {
    /// 客户端主动取消。
    ClientCancelled,
    /// 预算耗尽。
    BudgetExceeded,
    /// 设备/节点撤销。
    Revoked,
    /// 摘要不符。
    DigestMismatch,
}

/// `attachment.abort` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbortBody {
    #[serde(rename = "transferId")]
    pub transfer_id: Uuid,
    pub reason: AbortReason,
}

/// `attachment.read` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadBody {
    #[serde(rename = "attachmentId")]
    pub attachment_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub offset: DecimalString,
    #[serde(rename = "maxLength")]
    pub max_length: DecimalString,
}

/// `attachment.read_chunk` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadChunkBody {
    #[serde(rename = "attachmentId")]
    pub attachment_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub offset: DecimalString,
    pub length: DecimalString,
    #[serde(rename = "totalLength")]
    pub total_length: DecimalString,
    pub data: ChunkData,
    pub eof: bool,
}

/// `attachment.manifest_begin` 的 body：清单只绑定**已存在的正式会话**（顶层 `sessionId`）。
///
/// `submissionId` 是**附件清单**标识，`session.prompt` 用它引用已封存的清单。准备清单分支已被
/// 整体移除（`docs/SYNC_PROTOCOL.md` §3.4），出现 `target` 即解码失败。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestBeginBody {
    #[serde(rename = "submissionId")]
    pub submission_id: Uuid,
    #[serde(rename = "sessionId")]
    pub session_id: Uuid,
    pub subject: DeviceSubject,
    #[serde(rename = "attachmentCount")]
    pub attachment_count: DecimalString,
}

/// `attachment.manifest_page` 的 body（1..=64 个唯一 `transferId`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestPageBody {
    /// 清单标识。
    pub submission_id: Uuid,
    /// 本页的传输标识，顺序即清单顺序。
    pub transfer_ids: Vec<Uuid>,
    /// 是否最后一页。
    pub final_page: bool,
}

impl ManifestPageBody {
    fn validate(&self) -> Result<(), ValueError> {
        if self.transfer_ids.is_empty() || self.transfer_ids.len() > MANIFEST_PAGE_LIMIT {
            return Err(ValueError::TextLength {
                min: 1,
                max: MANIFEST_PAGE_LIMIT,
                actual: self.transfer_ids.len(),
            });
        }
        for (index, value) in self.transfer_ids.iter().enumerate() {
            if self.transfer_ids[..index].contains(value) {
                return Err(ValueError::RepeatedItem {
                    field: "transferIds",
                    value: value.as_str().to_owned(),
                });
            }
        }
        Ok(())
    }
}

impl Serialize for ManifestPageBody {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("submissionId", &self.submission_id)?;
        map.serialize_entry("transferIds", &self.transfer_ids)?;
        map.serialize_entry("final", &self.final_page)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for ManifestPageBody {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(rename = "submissionId")]
            submission_id: Uuid,
            #[serde(rename = "transferIds")]
            transfer_ids: Vec<Uuid>,
            #[serde(rename = "final")]
            final_page: bool,
        }
        let wire = Wire::deserialize(deserializer)?;
        let body = Self {
            submission_id: wire.submission_id,
            transfer_ids: wire.transfer_ids,
            final_page: wire.final_page,
        };
        body.validate().map_err(DeError::custom)?;
        Ok(body)
    }
}

/// `attachment.manifest_sealed` 的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestSealedBody {
    #[serde(rename = "submissionId")]
    pub submission_id: Uuid,
    #[serde(rename = "attachmentCount")]
    pub attachment_count: DecimalString,
    #[serde(rename = "manifestDigest")]
    pub manifest_digest: Base64Url<32>,
    #[serde(rename = "sealedAt")]
    pub sealed_at: Timestamp,
}

/// `attachment.error` 的 body（code 复用 §12.2 的错误码词表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: Text<1024>,
    pub retryable: bool,
    #[serde(rename = "transferId")]
    pub transfer_id: Nullable<Uuid>,
    pub details: RawObject,
}

impl ErrorBody {
    /// 按 registry 的语义填 `retryable`（调用方不得自行发明）。
    pub fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code,
            message: Text::parse(message).unwrap_or_else(|_| {
                // 固定说明文本远短于上限；兜底只为不 panic（仍是合法文本）。
                Text::parse("attachment error").expect("常量说明文本合法")
            }),
            retryable: code.default_retryable(),
            transfer_id: Nullable::null(),
            details: RawObject::empty(),
        }
    }

    /// 带传输标识与已登记 `details`。
    pub fn with_transfer(mut self, transfer: Uuid, details: RawObject) -> Self {
        self.transfer_id = Nullable::value(transfer);
        self.details = details;
        self
    }
}

/// 一帧的 body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum FrameBody {
    /// `attachment.begin`
    Begin(BeginBody),
    /// `attachment.chunk`
    Chunk(ChunkBody),
    /// `attachment.chunk_ack`
    ChunkAck(ChunkAckBody),
    /// `attachment.seal`
    Seal(SealBody),
    /// `attachment.sealed`
    Sealed(SealedBody),
    /// `attachment.status`
    Status(StatusBody),
    /// `attachment.abort`
    Abort(AbortBody),
    /// `attachment.read`
    Read(ReadBody),
    /// `attachment.read_chunk`
    ReadChunk(ReadChunkBody),
    /// `attachment.manifest_begin`
    ManifestBegin(ManifestBeginBody),
    /// `attachment.manifest_page`
    ManifestPage(ManifestPageBody),
    /// `attachment.manifest_sealed`
    ManifestSealed(ManifestSealedBody),
    /// `attachment.error`
    Error(ErrorBody),
}

impl FrameBody {
    /// 对应的帧类型。
    pub fn frame_type(&self) -> FrameType {
        match self {
            FrameBody::Begin(_) => FrameType::Begin,
            FrameBody::Chunk(_) => FrameType::Chunk,
            FrameBody::ChunkAck(_) => FrameType::ChunkAck,
            FrameBody::Seal(_) => FrameType::Seal,
            FrameBody::Sealed(_) => FrameType::Sealed,
            FrameBody::Status(_) => FrameType::Status,
            FrameBody::Abort(_) => FrameType::Abort,
            FrameBody::Read(_) => FrameType::Read,
            FrameBody::ReadChunk(_) => FrameType::ReadChunk,
            FrameBody::ManifestBegin(_) => FrameType::ManifestBegin,
            FrameBody::ManifestPage(_) => FrameType::ManifestPage,
            FrameBody::ManifestSealed(_) => FrameType::ManifestSealed,
            FrameBody::Error(_) => FrameType::Error,
        }
    }
}

/// 一条附件帧（信封 + body）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Frame {
    /// 发送方的消息标识（每条消息唯一）。
    #[serde(rename = "messageId")]
    pub message_id: Uuid,
    /// 连接标识（与 `auth.server_challenge` 下发的一致）。
    #[serde(rename = "connectionId")]
    pub connection_id: Uuid,
    /// 本方向的连接内序号（严格加一）。
    #[serde(rename = "connectionSequence")]
    pub connection_sequence: DecimalString,
    /// body。
    pub body: FrameBody,
}

impl Frame {
    /// 构造一帧。
    pub fn new(
        message_id: Uuid,
        connection_id: Uuid,
        connection_sequence: DecimalString,
        body: FrameBody,
    ) -> Self {
        Self {
            message_id,
            connection_id,
            connection_sequence,
            body,
        }
    }

    /// 帧类型。
    pub fn frame_type(&self) -> FrameType {
        self.body.frame_type()
    }

    /// 编成 wire 文本（`protocolVersion: 1` 固定）。
    pub fn encode(&self) -> Result<String, serde_json::Error> {
        encode(
            &self.message_id,
            &self.connection_id,
            &self.connection_sequence,
            &self.body,
        )
    }

    /// 从 wire 文本解码。
    ///
    /// 只接受 `protocolVersion: 1`、已知 `type` 与严格 body 形状；`connectionSequence` 必须在 v1 的
    /// 64 位有符号上界内（`§3.3`：超界是 `schema_invalid`，不截断）。
    pub fn decode(text: &str) -> Result<Self, DecodeError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope {
            #[serde(rename = "protocolVersion")]
            protocol_version: u64,
            #[serde(rename = "type")]
            _frame_type: String,
            #[serde(rename = "messageId")]
            message_id: Uuid,
            #[serde(rename = "connectionId")]
            connection_id: Uuid,
            #[serde(rename = "connectionSequence")]
            connection_sequence: DecimalString,
            body: Box<serde_json::value::RawValue>,
        }

        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|_| DecodeError::InvalidJson)?;
        // 先看版本与 type 的**原始文本**，再解码整个信封：顺序让调用方能区分
        // `version_unsupported`/`type_unsupported` 与真正的形状错误。
        if value
            .get("protocolVersion")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|version| version != 1)
        {
            return Err(DecodeError::VersionUnsupported);
        }
        let frame_type = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or(DecodeError::Schema)?;
        let frame_type =
            FrameType::from_str(frame_type).map_err(|()| DecodeError::TypeUnsupported)?;
        let envelope: Envelope = serde_json::from_str(text).map_err(|_| DecodeError::Schema)?;
        if envelope.protocol_version != 1 {
            return Err(DecodeError::VersionUnsupported);
        }
        validate_sequence(&envelope.connection_sequence)?;
        let body = decode_body(frame_type, envelope.body.get())?;
        Ok(Self {
            message_id: envelope.message_id,
            connection_id: envelope.connection_id,
            connection_sequence: envelope.connection_sequence,
            body,
        })
    }
}

fn validate_sequence(sequence: &DecimalString) -> Result<(), DecodeError> {
    let value: u64 = sequence.as_str().parse().map_err(|_| DecodeError::Schema)?;
    if value > MAX_SEQUENCE {
        return Err(DecodeError::Schema);
    }
    Ok(())
}

fn decode_body(frame_type: FrameType, body: &str) -> Result<FrameBody, DecodeError> {
    fn parse<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, DecodeError> {
        serde_json::from_str(body).map_err(|_| DecodeError::Schema)
    }
    Ok(match frame_type {
        FrameType::Begin => FrameBody::Begin(parse(body)?),
        FrameType::Chunk => FrameBody::Chunk(parse(body)?),
        FrameType::ChunkAck => FrameBody::ChunkAck(parse(body)?),
        FrameType::Seal => FrameBody::Seal(parse(body)?),
        FrameType::Sealed => FrameBody::Sealed(parse(body)?),
        FrameType::Status => FrameBody::Status(parse(body)?),
        FrameType::Abort => FrameBody::Abort(parse(body)?),
        FrameType::Read => FrameBody::Read(parse(body)?),
        FrameType::ReadChunk => FrameBody::ReadChunk(parse(body)?),
        FrameType::ManifestBegin => FrameBody::ManifestBegin(parse(body)?),
        FrameType::ManifestPage => FrameBody::ManifestPage(parse(body)?),
        FrameType::ManifestSealed => FrameBody::ManifestSealed(parse(body)?),
        FrameType::Error => FrameBody::Error(parse(body)?),
    })
}

/// 帧的方向（供适配器做「收到不该由本端发送的帧」判定；协议本身不因方向改变形状）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// 上传方 → 接收方（`begin`/`chunk`/`seal`/`manifest_*`）。
    Upload,
    /// 接收方 → 上传方（`chunk_ack`/`sealed`）。
    Ack,
    /// 双向（`status`/`abort`/`read`/`read_chunk`/`error`）。
    Both,
}

impl FrameType {
    /// 该帧在 §3.4 帧表里的语义方向。
    pub fn direction(self) -> Direction {
        match self {
            FrameType::Begin
            | FrameType::Chunk
            | FrameType::Seal
            | FrameType::ManifestBegin
            | FrameType::ManifestPage
            | FrameType::ManifestSealed => Direction::Upload,
            FrameType::ChunkAck | FrameType::Sealed => Direction::Ack,
            FrameType::Status
            | FrameType::Abort
            | FrameType::Read
            | FrameType::ReadChunk
            | FrameType::Error => Direction::Both,
        }
    }
}

/// 把协议版本固定为 1 的类型别名（调用方组装信封时的显式来源）。
pub type ProtocolVersion = ProtocolVersionV1;

/// 把一帧编成 wire 文本（`protocolVersion: 1` 固定）。
///
/// 与 [`Frame::encode`] 同一形状，但不要求调用方先构造拥有 body 的 [`Frame`]：服务端回帧时 body 已经
/// 是构造好的 `FrameBody`（可能携带 32 KiB 数据），再克隆一次只为装箱没有必要。
pub fn encode(
    message_id: &Uuid,
    connection_id: &Uuid,
    connection_sequence: &DecimalString,
    body: &FrameBody,
) -> Result<String, serde_json::Error> {
    #[derive(Serialize)]
    struct Wire<'a> {
        #[serde(rename = "protocolVersion")]
        protocol_version: u8,
        #[serde(rename = "type")]
        frame_type: &'static str,
        #[serde(rename = "messageId")]
        message_id: &'a Uuid,
        #[serde(rename = "connectionId")]
        connection_id: &'a Uuid,
        #[serde(rename = "connectionSequence")]
        connection_sequence: &'a DecimalString,
        body: &'a FrameBody,
    }
    serde_json::to_string(&Wire {
        protocol_version: 1,
        frame_type: body.frame_type().as_str(),
        message_id,
        connection_id,
        connection_sequence,
        body,
    })
}
