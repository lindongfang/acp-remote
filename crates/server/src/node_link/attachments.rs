//! Node Link 独立附件连接（`docs/NODE_LINK_PROTOCOL.md` §12.9）：`/node-link/v1/attachments` 的帧适配与认证后的分派。
//!
//! 连接面的握手、endpoint 绑定、撤销、心跳与关闭语义完全复用 [`crate::node_link::conn`]（同一个
//! [`NodeLinkConn`](crate::node_link::conn::NodeLinkConn)，只是 [`Purpose::Attachments`](crate::node_link::conn::Purpose)）；
//! 本模块只负责三件事：
//!
//! 1. **协商门控**：`node.challenge.selectedFeatures` 里没有 `node-link.attachment-transfer.v1` 时，
//!    任何 `attachment.*` 帧都回 `nodelink.attachment.negotiation_required` 并以 `4400` 关闭（§12.9）。
//!    门控在**进入引擎之前**判定，未协商的连接不触碰存储、不产生任何写入。
//! 2. **方向判定**：Owner 只接受上传方向的帧（`begin`/`chunk`/`seal`/`manifest_*`）与双向帧
//!    （`status`/`abort`/`read`/`error`）。对端发来 `chunk_ack`/`sealed`/`read_chunk` 属于方向错误，
//!    回 `nodelink.protocol.type_unsupported`；不静默丢弃，也不当成业务命令。
//! 3. **协议中立引擎的 wire 映射**：帧 → [`Request`]，[`Reply`]/[`Fault`] → 帧与错误体。分块/封存/清单
//!    的判定只在 [`crate::attachment_transfer`] 里有一份实现。
//!
//! 连接**不登记业务注册表**：附件连接不进 `ConnectionRegistry::handles()`（WP5 的订阅者集合），撤销经
//! [`ConnectionRegistry::close_node`](crate::node_link::conn::ConnectionRegistry::close_node) 同时关闭业务与
//! 附件两张表。出站序号与队列预算与业务消息共用同一份账本（`ConnectionHandle::send_attachment`）。
//!
//! 依赖纪律：只调用 `core::ports` 的附件端口、`core::use_cases` 的只读装配入口与
//! `node-link-protocol` 的公开 DTO；不 import `storage-sqlite`、不调用 `server::sync`。
use std::sync::Arc;

use acp_core::model::{AttachmentId, Digest, SessionId, SubmissionId, Timestamp};
use acp_core::use_cases::UseCases;
use node_link_protocol::attachment::{
    AbortReason as WireAbortReason, BeginBody, Direction, ErrorBody, Frame, FrameBody, FrameType,
    ManifestBeginBody, ManifestSealedBody, ReadChunkBody, SealedBody, StatusBody, TransferState,
};
use node_link_protocol::common::{
    DecimalString, Nullable, RawObject, Timestamp as WireTimestamp, Uuid,
};
use node_link_protocol::error::ErrorCode;
use tracing::{debug, warn};

use crate::attachment_transfer::{
    AbortReason, BeginRequest, Engine, Fault, Reply, Request, Subject, TransferStatus,
};
use crate::node_link::conn::{ConnectionHandle, close};

/// `node-link.attachment-transfer.v1`（`compatibility/features/v1/features.json`）。
pub const ATTACHMENT_FEATURE: &str = "node-link.attachment-transfer.v1";

/// 附件连接的分派结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentOutcome {
    /// 已处理（回复或错误经会话句柄入队）。
    Claimed,
    /// 本连接的附件连接没有装配引擎：显式内部不可用，不静默丢弃。
    Unavailable,
    /// 以该 close code 结束连接（未协商 `4400`）。
    Close(u16, &'static str),
}

/// 认证后附件帧的分派口（组合根把它装配进 `Purpose::Attachments` 的 `SyncConn`）。
#[async_trait::async_trait]
pub trait AttachmentRoute: Send + Sync + 'static {
    /// 处理一条已通过信封/序号校验的附件帧。
    async fn route(
        &self,
        session: &ConnectionHandle,
        features: &[String],
        frame: &Frame,
    ) -> AttachmentOutcome;

    /// 附件连接关闭（正常关闭、断链、撤销与 daemon 关闭都经由同一收尾路径）。
    ///
    /// 引擎在这里释放该连接预留的活动传输槽位：登记若留在进程内的 `receiving` 集合里，Access 放弃的
    /// 上传会永久占住单节点 8 个活动传输的预算。默认实现为空操作（不需要连接期账的路由不必实现）。
    fn released(&self, _session: &ConnectionHandle) {}
}

/// Sync 面的附件服务：持有协议中立引擎，负责门控与 wire 映射。
pub struct AttachmentService {
    engine: Arc<Engine>,
}

impl std::fmt::Debug for AttachmentService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("node_link::AttachmentService")
            .finish_non_exhaustive()
    }
}

impl AttachmentService {
    /// 从用例面取附件端口与时钟装配。
    ///
    /// `None` = 本进程没有装配 `AttachmentTransferStore`：调用方应当**不**注册附件端点（能力协商也不
    /// 宣告该 feature），而不是注册一个每条帧都回内部错误的端点。
    pub fn from_core(core: &Arc<UseCases>) -> Option<Arc<Self>> {
        let transfers = Arc::clone(core.attachment_transfers()?);
        let clock = Arc::clone(core.clock());
        Some(Arc::new(Self {
            engine: Engine::new(transfers, clock),
        }))
    }

    /// 从已装配的引擎构造（组合根在 Sync 与 Node Link 之间共享同一个引擎时使用）。
    pub fn from_engine(engine: Arc<Engine>) -> Arc<Self> {
        Arc::new(Self { engine })
    }
}

#[async_trait::async_trait]
impl AttachmentRoute for AttachmentService {
    async fn route(
        &self,
        session: &ConnectionHandle,
        features: &[String],
        frame: &Frame,
    ) -> AttachmentOutcome {
        if !features.iter().any(|feature| feature == ATTACHMENT_FEATURE) {
            warn!(
                event = "node_link.attachment_negotiation_required",
                connection_id = session.connection_id().as_str(),
                "an attachment frame arrived on a connection that did not negotiate the feature"
            );
            send_error(
                session,
                frame.transfer_hint(),
                ErrorCode::AttachmentNegotiationRequired,
                "the connection did not negotiate node-link.attachment-transfer.v1",
                RawObject::empty(),
            );
            return AttachmentOutcome::Close(
                close::PROTOCOL,
                "attachment feature was not negotiated",
            );
        }
        // 主体来自**已认证的节点身份**，不取自帧里自报的 subject（跨节点冒用在引擎里按
        // binding_mismatch 拒绝）。
        let subject = Subject::Node(session.node_id().as_str().to_owned());
        let request = match to_request(frame) {
            Ok(Some(request)) => request,
            // 客户端上报的 `attachment.error`：只记诊断，不产生回帧（协议没有 error-of-error）。
            Ok(None) => return AttachmentOutcome::Claimed,
            Err(Outcome::Unsupported) => {
                send_error(
                    session,
                    frame.transfer_hint(),
                    ErrorCode::ProtocolTypeUnsupported,
                    "this frame direction is not accepted on an attachment connection",
                    RawObject::empty(),
                );
                return AttachmentOutcome::Claimed;
            }
            Err(Outcome::Invalid) => {
                send_error(
                    session,
                    frame.transfer_hint(),
                    ErrorCode::ProtocolSchemaInvalid,
                    "the attachment body cannot be used",
                    RawObject::empty(),
                );
                return AttachmentOutcome::Claimed;
            }
        };
        match self
            .engine
            .handle(session.connection_id().as_str(), &subject, request)
            .await
        {
            Ok(Some(reply)) => {
                if let Some(body) = to_body(reply) {
                    if session.send_attachment(&body).is_err() {
                        debug!(
                            event = "node_link.attachment_reply_dropped",
                            connection_id = session.connection_id().as_str(),
                            "the attachment reply could not be queued"
                        );
                    }
                }
                AttachmentOutcome::Claimed
            }
            Ok(None) => AttachmentOutcome::Claimed,
            Err(fault) => {
                let (code, message, details) = wire_fault(&fault);
                send_error(session, frame.transfer_hint(), code, message, details);
                AttachmentOutcome::Claimed
            }
        }
    }

    fn released(&self, session: &ConnectionHandle) {
        self.engine
            .release_connection(session.connection_id().as_str());
    }
}

/// 帧 → 中立请求的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// 方向不允许（服务端从客户端接收的帧集合）。
    Unsupported,
    /// body 形状不可用（解码已通过，但数值越界等）。
    Invalid,
}

/// 客户端可发送的帧类型（方向判定的唯一清单）。
fn is_client_frame(frame_type: FrameType) -> bool {
    match frame_type.direction() {
        Direction::Upload => true,
        Direction::Ack => false,
        // 双向帧里，`read_chunk` 只是接收方对 `read` 的回复；客户端发它是方向错误。
        Direction::Both => frame_type != FrameType::ReadChunk,
    }
}

fn to_request(frame: &Frame) -> Result<Option<Request>, Outcome> {
    let frame_type = frame.body.frame_type();
    if !is_client_frame(frame_type) {
        return Err(Outcome::Unsupported);
    }
    let request = match &frame.body {
        FrameBody::Begin(body) => Request::Begin(begin_request(body)?),
        FrameBody::Chunk(body) => Request::Chunk {
            transfer: attachment_id(&body.transfer_id),
            offset: decimal(&body.offset)?,
            data: body.data.as_bytes().to_vec(),
        },
        FrameBody::Seal(body) => Request::Seal {
            transfer: attachment_id(&body.transfer_id),
            length: decimal(&body.length)?,
            sha256: wire_digest(&body.sha256)?,
        },
        FrameBody::Status(body) => {
            require_status_shape(body)?;
            Request::Status {
                transfer: attachment_id(&body.transfer_id),
            }
        }
        FrameBody::Abort(body) => Request::Abort {
            transfer: attachment_id(&body.transfer_id),
            reason: abort_reason(body.reason),
        },
        FrameBody::Read(body) => Request::Read {
            attachment: attachment_id(&body.attachment_id),
            session: session_id(&body.session_id)?,
            offset: decimal(&body.offset)?,
            max_length: decimal(&body.max_length)?,
        },
        FrameBody::ManifestBegin(body) => manifest_begin(body)?,
        FrameBody::ManifestPage(body) => Request::ManifestPage {
            submission: submission_id(&body.submission_id),
            transfer_ids: body.transfer_ids.iter().map(attachment_id).collect(),
            final_page: body.final_page,
        },
        FrameBody::ManifestSealed(body) => Request::ManifestSealed {
            submission: submission_id(&body.submission_id),
            manifest_digest: wire_digest(&body.manifest_digest)?,
        },
        FrameBody::Error(_) => return Ok(None),
        // 方向判定（`is_client_frame`）已拒绝的方向；这里显式列出以免新增帧类型时静默落入通配。
        FrameBody::ChunkAck(_) | FrameBody::Sealed(_) | FrameBody::ReadChunk(_) => {
            return Err(Outcome::Unsupported);
        }
    };
    Ok(Some(request))
}

fn begin_request(body: &BeginBody) -> Result<BeginRequest, Outcome> {
    // Node Link 没有 `core.session-preparation.v1`：准备目标不是本协议的合法形状（不静默当成正式上传）。
    let BeginBody::Owned(body) = body else {
        return Err(Outcome::Invalid);
    };
    Ok(BeginRequest {
        transfer: attachment_id(&body.transfer_id),
        session: session_id(&body.session_id)?,
        subject: Subject::Node(body.subject.id.as_str().to_owned()),
        total_length: decimal(&body.total_length)?,
        chunk_size: body.chunk_size.get(),
        media_type: body.media_type.as_str().to_owned(),
        file_name: body.file_name.as_str().to_owned(),
    })
}

fn manifest_begin(body: &ManifestBeginBody) -> Result<Request, Outcome> {
    let ManifestBeginBody::Owned(body) = body else {
        return Err(Outcome::Invalid);
    };
    Ok(Request::ManifestBegin {
        submission: submission_id(&body.submission_id),
        session: session_id(&body.session_id)?,
        subject: Subject::Node(body.subject.id.as_str().to_owned()),
        attachment_count: decimal(&body.attachment_count)?,
    })
}

/// `attachment.status` 的 `state`/`nextOffset`/`sealedSha256` 是**服务端**回答的内容；客户端发来的
/// 状态帧只有 `transferId` 有意义，其余字段必须与它自报的状态一致才算是合法请求（协议不禁止客户端
/// 报告状态，但服务端从不把它当作事实）。
fn require_status_shape(body: &StatusBody) -> Result<(), Outcome> {
    let _ = (&body.state, &body.received_length, &body.next_offset);
    match body.state {
        TransferState::Receiving
        | TransferState::Sealed
        | TransferState::Aborted
        | TransferState::Expired => Ok(()),
    }
}

fn abort_reason(reason: WireAbortReason) -> AbortReason {
    match reason {
        WireAbortReason::ClientCancelled => AbortReason::ClientCancelled,
        WireAbortReason::BudgetExceeded => AbortReason::BudgetExceeded,
        WireAbortReason::Revoked => AbortReason::Revoked,
        WireAbortReason::DigestMismatch => AbortReason::DigestMismatch,
    }
}

fn attachment_id(id: &Uuid) -> AttachmentId {
    AttachmentId::new(id.as_str()).expect("wire uuid is a valid attachment id")
}

fn session_id(id: &Uuid) -> Result<SessionId, Outcome> {
    SessionId::new(id.as_str()).map_err(|_| Outcome::Invalid)
}

fn submission_id(id: &Uuid) -> SubmissionId {
    SubmissionId::new(id.as_str()).expect("wire uuid is a valid submission id")
}

fn decimal(value: &DecimalString) -> Result<u64, Outcome> {
    value.as_str().parse::<u64>().map_err(|_| Outcome::Invalid)
}

/// wire 的定长 base64url 字段 → core 的规范摘要文本（两者校验口径一致，这里只做编码转换）。
fn wire_digest(value: &node_link_protocol::common::Base64Url<32>) -> Result<Digest, Outcome> {
    use base64::Engine as _;
    let text = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.as_bytes());
    Digest::new(&text).map_err(|_| Outcome::Invalid)
}

/// 中立回复 → wire body（`None` = 该回复不应发送）。
fn to_body(reply: Reply) -> Option<FrameBody> {
    Some(match reply {
        Reply::ChunkAck {
            transfer,
            offset,
            length,
            duplicate,
            next_offset,
        } => FrameBody::ChunkAck(node_link_protocol::attachment::ChunkAckBody {
            transfer_id: uuid(&transfer)?,
            offset: decimal_string(offset),
            length: decimal_string(length),
            duplicate,
            next_offset: decimal_string(next_offset),
        }),
        Reply::Sealed {
            transfer,
            attachment,
            length,
            sha256,
            sealed_at,
        } => FrameBody::Sealed(SealedBody {
            transfer_id: uuid(&transfer)?,
            attachment_id: uuid(&attachment)?,
            length: decimal_string(length),
            sha256: base64url(&sha256)?,
            sealed_at: wire_timestamp(&sealed_at)?,
        }),
        Reply::Status(status) => FrameBody::Status(status_body(status)?),
        Reply::ReadChunk {
            attachment,
            session,
            offset,
            length,
            total_length,
            data,
            eof,
        } => FrameBody::ReadChunk(ReadChunkBody {
            attachment_id: uuid(&attachment)?,
            session_id: uuid(&session)?,
            offset: decimal_string(offset),
            length: decimal_string(length),
            total_length: decimal_string(total_length),
            data: node_link_protocol::attachment::ChunkData::from_bytes(data).ok()?,
            eof,
        }),
        Reply::ManifestSealed {
            submission,
            attachment_count,
            manifest_digest,
            sealed_at,
        } => FrameBody::ManifestSealed(ManifestSealedBody {
            submission_id: uuid(&submission)?,
            attachment_count: decimal_string(attachment_count),
            manifest_digest: base64url(&manifest_digest)?,
            sealed_at: wire_timestamp(&sealed_at)?,
        }),
    })
}

fn status_body(status: TransferStatus) -> Option<StatusBody> {
    Some(StatusBody {
        transfer_id: uuid(&status.transfer)?,
        state: match status.state {
            acp_core::model::AttachmentTransferState::Receiving => TransferState::Receiving,
            acp_core::model::AttachmentTransferState::Sealed => TransferState::Sealed,
            acp_core::model::AttachmentTransferState::Aborted => TransferState::Aborted,
            acp_core::model::AttachmentTransferState::Expired => TransferState::Expired,
        },
        received_length: decimal_string(status.received_length),
        next_offset: decimal_string(status.next_offset),
        sealed_sha256: match status.sealed_sha256 {
            Some(digest) => Nullable::value(base64url(&digest)?),
            None => Nullable::null(),
        },
    })
}

fn uuid(id: impl AsRef<str>) -> Option<Uuid> {
    Uuid::parse(id.as_ref()).ok()
}

fn decimal_string(value: u64) -> DecimalString {
    DecimalString::parse(&value.to_string()).expect("u64 is a canonical decimal string")
}

fn base64url(digest: &Digest) -> Option<node_link_protocol::common::Base64Url<32>> {
    node_link_protocol::common::Base64Url::<32>::parse(digest.as_str()).ok()
}

fn wire_timestamp(value: &Timestamp) -> Option<WireTimestamp> {
    WireTimestamp::parse(value.as_str()).ok()
}

/// 中立失败 → wire 错误码、说明与 `details`。
fn wire_fault(fault: &Fault) -> (ErrorCode, &'static str, RawObject) {
    match fault {
        Fault::NegotiationRequired => (
            ErrorCode::AttachmentNegotiationRequired,
            "the connection did not negotiate node-link.attachment-transfer.v1",
            RawObject::empty(),
        ),
        Fault::TransferNotFound => (
            ErrorCode::AttachmentTransferNotFound,
            "the transfer or submission does not exist",
            RawObject::empty(),
        ),
        Fault::BindingMismatch => (
            ErrorCode::AttachmentBindingMismatch,
            "the transfer does not belong to this session or subject",
            RawObject::empty(),
        ),
        Fault::OffsetMismatch { expected, actual } => (
            ErrorCode::AttachmentOffsetMismatch,
            "the chunk offset does not match the next expected offset",
            details(&serde_json::json!({
                "expectedOffset": expected.to_string(),
                "actualOffset": actual.to_string(),
            })),
        ),
        Fault::ChunkConflict { offset } => (
            ErrorCode::AttachmentChunkConflict,
            "another chunk was already accepted at this offset",
            details(&serde_json::json!({ "offset": offset.to_string() })),
        ),
        Fault::DigestMismatch => (
            ErrorCode::AttachmentDigestMismatch,
            "the length or digest does not match the received bytes",
            RawObject::empty(),
        ),
        Fault::Sealed => (
            ErrorCode::AttachmentSealed,
            "the transfer or manifest is already sealed",
            RawObject::empty(),
        ),
        Fault::BudgetExhausted => (
            ErrorCode::AttachmentBudgetExhausted,
            "the transfer or connection budget is exhausted",
            RawObject::empty(),
        ),
        Fault::Invalid => (
            ErrorCode::ProtocolSchemaInvalid,
            "the attachment request is not acceptable",
            RawObject::empty(),
        ),
        Fault::Unavailable => (
            ErrorCode::InternalUnavailable,
            "the attachment storage is unavailable",
            RawObject::empty(),
        ),
    }
}

fn details(value: &serde_json::Value) -> RawObject {
    RawObject::parse(&value.to_string()).unwrap_or_else(|_| RawObject::empty())
}

/// 发一条 `attachment.error`（尽力而为：入队失败只记诊断，不改变调用方的判定）。
fn send_error(
    session: &ConnectionHandle,
    transfer: Option<Uuid>,
    code: ErrorCode,
    message: &'static str,
    details: RawObject,
) {
    let body = match transfer {
        Some(transfer) => ErrorBody::new(code, message).with_transfer(transfer, details),
        None => {
            let mut body = ErrorBody::new(code, message);
            body.details = details;
            body
        }
    };
    if session.send_attachment(&FrameBody::Error(body)).is_err() {
        debug!(
            event = "node_link.attachment_error_dropped",
            connection_id = session.connection_id().as_str(),
            code = code.as_str(),
            "the attachment error could not be queued"
        );
    }
}

/// 帧的属性读取：错误回帧要带 `transferId`（能从 body 判定时）。
trait TransferHint {
    fn transfer_hint(&self) -> Option<Uuid>;
}

impl TransferHint for Frame {
    fn transfer_hint(&self) -> Option<Uuid> {
        match &self.body {
            FrameBody::Begin(body) => Some(body.transfer_id.clone()),
            FrameBody::Chunk(body) => Some(body.transfer_id.clone()),
            FrameBody::Seal(body) => Some(body.transfer_id.clone()),
            FrameBody::Status(body) => Some(body.transfer_id.clone()),
            FrameBody::Abort(body) => Some(body.transfer_id.clone()),
            FrameBody::Read(body) => Some(body.attachment_id.clone()),
            FrameBody::ManifestBegin(body) => Some(body.submission_id.clone()),
            FrameBody::ManifestPage(body) => Some(body.submission_id.clone()),
            FrameBody::ManifestSealed(body) => Some(body.submission_id.clone()),
            FrameBody::ChunkAck(_)
            | FrameBody::Sealed(_)
            | FrameBody::ReadChunk(_)
            | FrameBody::Error(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Node Link 侧的帧映射回归：`begin` 的主体是**节点**（不是设备），确认方向判定先行。

    use super::*;
    use node_link_protocol::attachment::{ChunkAckBody, NodeSubject};
    use node_link_protocol::common::NonEmptyText;

    fn uuid() -> Uuid {
        Uuid::parse(&uuid::Uuid::new_v4().to_string()).expect("uuid")
    }

    fn frame(body: FrameBody) -> Frame {
        Frame::new(
            uuid(),
            uuid(),
            DecimalString::parse("1").expect("sequence"),
            body,
        )
    }

    #[test]
    fn begin_maps_to_a_node_subject() {
        let subject = uuid();
        let frame = frame(FrameBody::Begin(BeginBody::Owned(
            node_link_protocol::attachment::OwnedBeginBody {
                transfer_id: uuid(),
                session_id: uuid(),
                subject: NodeSubject::new(subject.clone()),
                total_length: DecimalString::parse("4").expect("length"),
                chunk_size: node_link_protocol::attachment::FixedChunkSize::new(32768)
                    .expect("chunk size"),
                media_type: NonEmptyText::<255>::parse("text/plain").expect("media type"),
                file_name: NonEmptyText::<255>::parse("note.txt").expect("file name"),
            },
        )));
        match to_request(&frame)
            .expect("decoded request")
            .expect("request")
        {
            Request::Begin(begin) => {
                // Node Link 的主体种类必须是 `Node`：同一 UUID 在 Sync 面是设备，不能混用预算键。
                assert_eq!(begin.subject, Subject::Node(subject.as_str().to_owned()));
            }
            other => panic!("expected a begin request, got {other:?}"),
        }
    }

    #[test]
    fn ack_direction_frames_are_not_accepted_from_the_peer() {
        let frame = frame(FrameBody::ChunkAck(ChunkAckBody {
            transfer_id: uuid(),
            offset: DecimalString::parse("0").expect("offset"),
            length: DecimalString::parse("0").expect("length"),
            duplicate: false,
            next_offset: DecimalString::parse("0").expect("offset"),
        }));
        assert!(matches!(to_request(&frame), Err(Outcome::Unsupported)));
    }
}
