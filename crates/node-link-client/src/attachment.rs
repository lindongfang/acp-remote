//! Access → Owner 的有界内存附件转发（`docs/NODE_LINK_PROTOCOL.md` §12.9）。
//!
//! 本模块是**客户端**侧的 `attachment.*` 帧族实现：在业务连接之外，另建一条已认证的
//! `/node-link/v1/attachments` 连接（握手复用 [`crate::handshake`]），把本地客户端的分块上传与读取
//! 原样转发给 Owner。
//!
//! 三条不可让步的边界：
//!
//! 1. **不落盘**：本模块只持有一条有界队列（[`QUEUE_CAPACITY`] 个请求）、一个单块读缓冲
//!    （[`CHUNK_SIZE`] 字节）与流式哈希上下文。正文从不写入临时文件、SQLite、日志或持久缓存；
//!    日志只带传输标识与错误码。`upload` 从 [`AsyncRead`] 边读边发，内存工作集与文件大小无关。
//! 2. **停等**：一个请求在途时才发下一帧（单连接的请求/响应严格串行）。因此「每传输最多一块未确认」
//!    与「队列有界」是结构性的，而不是靠调用方自觉；单块上限由 wire 类型强制（[`CHUNK_SIZE`]）。
//! 3. **已转发不等于已提交**：`seal` 成功只表示 Owner 封存了字节；业务提交（`session.prompt` 的清单
//!    绑定）是另一条命令，本模块不把它当成已完成。
//!
//! 连接所有权：一个 [`AttachmentForwarder`] 拥有一个运行任务（独占读写两半）与一个 JoinHandle。
//! [`AttachmentForwarder::close`] 触发关闭并等待任务结束；`Drop` 只发关闭信号（不在 runtime 之外等待）。
//! 断线、撤销与超时都会以显式错误结束在途请求，绝不自动重发有副作用的帧。
//!
//! 唯一权威：`docs/NODE_LINK_PROTOCOL.md` §12.9 与 `crates/node-link-protocol/src/attachment.rs`。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use acp_core::model::{
    AttachmentId, AttachmentTransferState, Digest, NodeId, SessionId, SubmissionId, Timestamp,
};
use node_link_protocol::attachment::{
    AbortBody, AbortReason as WireAbortReason, BeginBody, ChunkAckBody, ChunkBody, ChunkData,
    ErrorBody, Frame, FrameBody, ManifestBeginBody, ManifestPageBody, ManifestSealedBody,
    NodeSubject, ReadBody, ReadChunkBody, SealBody, SealedBody, StatusBody, TransferState,
};
use node_link_protocol::common::{
    Base64Url, DecimalString, NonEmptyText, Nullable, Timestamp as WireTimestamp, Uuid,
};
use node_link_protocol::envelope::{Envelope, MessageType};
use node_link_protocol::error::{ErrorCode, Ping};
use tokio::io::{AsyncRead, AsyncReadExt};

/// 当前在途请求的回复槽（停等：同一时刻最多一个）。
type PendingReply = Option<(
    Expect,
    tokio::sync::oneshot::Sender<Result<Option<FrameBody>, ClientError>>,
)>;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::error::ClientError;
use crate::handshake::{Established, wire_connection_fields};
use crate::manager::OwnerLink;
use crate::transport::{Incoming, MessageSink, MessageSource};

/// 发送队列容量（请求个数）。队列只承载「待发的一帧 + 回复通道」。
pub const QUEUE_CAPACITY: usize = 4;

/// 单次请求的等待上限。超过即认为连接状态不可判定，主动关闭该附件连接（不重发）。
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(120);

/// 单块原始数据上限（wire 固定常量，与协议 crate 同源）。
pub const CHUNK_SIZE: usize = node_link_protocol::attachment::CHUNK_SIZE;

/// 一次已封存附件的事实（`attachment.sealed`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedAttachment {
    /// 封存长度。
    pub length: u64,
    /// 整段流式 SHA-256（core 的规范文本）。
    pub sha256: Digest,
    /// Owner 的封存时间。
    pub sealed_at: Timestamp,
}

/// 一次传输的状态（`attachment.status`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferStatus {
    /// 当前状态。
    pub state: AttachmentTransferState,
    /// 已接收长度。
    pub received_length: u64,
    /// 下一期望偏移。
    pub next_offset: u64,
    /// 已封存摘要。
    pub sealed_sha256: Option<Digest>,
}

/// 一段已读取的附件数据（`attachment.read_chunk`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadChunk {
    /// 本段偏移。
    pub offset: u64,
    /// 本段长度。
    pub length: u64,
    /// 附件总长度。
    pub total_length: u64,
    /// 本段原始字节。
    pub data: Vec<u8>,
    /// 是否已到末尾。
    pub eof: bool,
}

/// 清单封存事实（`attachment.manifest_sealed`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedManifest {
    /// 清单内附件数。
    pub attachment_count: u64,
    /// 清单摘要（core 的规范文本）。
    pub manifest_digest: Digest,
    /// Owner 的封存时间。
    pub sealed_at: Timestamp,
}

/// 一次 `upload` 的终态：封存事实与源数据的实际长度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadOutcome {
    /// 封存事实。
    pub sealed: SealedAttachment,
    /// 实际读出的字节数（必须等于声明的 `total_length`）。
    pub sent: u64,
}

/// 附件转发句柄。
pub struct AttachmentForwarder {
    tx: mpsc::Sender<Operation>,
    /// 本连接在 Owner 上被认证的节点标识（`begin`/`manifest_begin` 的 `subject.id`）。
    subject: NodeId,
    /// 任务句柄；`close` 取出并 await。
    task: Mutex<Option<JoinHandle<()>>>,
    /// 是否已请求关闭（`Drop` 与 `close` 幂等）。
    closed: AtomicBool,
    /// 本连接的关闭信号：`upload` 在等待本地源时也要能被 `revoke`/`shutdown` 打断，
    /// 而不是只在下一次发送帧时才发现连接已经没了。
    shutdown: watch::Receiver<bool>,
    /// 测试注入路径的取消通道发送端：只有它存活，运行任务才不会把「发送端被丢弃」误判为取消。
    /// 生产路径使用 [`OwnerLink`] 的共享关闭通道，因此这里是 `None`。
    _cancel: Option<watch::Sender<bool>>,
}

impl std::fmt::Debug for AttachmentForwarder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AttachmentForwarder")
            .field("subject", &self.subject)
            .field("closed", &self.closed.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

/// 请求一条回复时要匹配的帧族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// `attachment.chunk_ack`。
    ChunkAck,
    /// `attachment.sealed`。
    Sealed,
    /// `attachment.status`（`status` 与 `abort` 的回复）。
    Status,
    /// `attachment.read_chunk`。
    ReadChunk,
    /// `attachment.manifest_sealed`。
    ManifestSealed,
}

enum Operation {
    /// 发送一帧；`expect` 为 `Some` 时等待对应回复。
    Frame {
        body: FrameBody,
        expect: Option<Expect>,
        reply: oneshot::Sender<Result<Option<FrameBody>, ClientError>>,
    },
    /// 关闭连接并结束任务。
    Close,
}

impl AttachmentForwarder {
    /// 建立一条新的附件连接（每次调用一条连接，用完即关）。
    ///
    /// 未协商 `node-link.attachment-transfer.v1` 时返回 [`ClientError::Unsupported`]，**不**建立连接：
    /// Owner 会在第一条附件帧上以 `negotiation_required` + `4400` 结束，提前失败避免触碰任何存储。
    pub async fn connect(owner: &OwnerLink) -> Result<Self, ClientError> {
        let (sink, source, established, shutdown) = owner.open_attachment_connection().await?;
        if !established.has(crate::handshake::ATTACHMENT_FEATURE) {
            return Err(ClientError::Unsupported);
        }
        let subject = owner.access_node_id();
        Ok(Self::from_parts_with_cancel(
            sink,
            source,
            established,
            subject,
            shutdown,
            None,
        ))
    }

    /// 从已建立的读写两半装配（测试注入脚本化 Owner；生产路径经 [`Self::connect`]）。
    ///
    /// `subject` 必须等于本连接在 Owner 上被认证的节点标识——Owner 校验 `begin` 的 `subject.id`
    /// 与已认证身份逐字相等，跨节点冒用按 `binding_mismatch` 拒绝。
    ///
    /// 这条路径的取消信号来自本句柄自身：`close`/`Drop` 已经通过队列传达关闭，运行任务不需要
    /// 外部通道（但仍持有发送端，避免「无发送者」被误判成取消）。
    pub fn from_parts(
        sink: Box<dyn MessageSink>,
        source: Box<dyn MessageSource>,
        established: Established,
        subject: NodeId,
    ) -> Self {
        let (cancel, cancel_rx) = watch::channel(false);
        Self::from_parts_with_cancel(sink, source, established, subject, cancel_rx, Some(cancel))
    }

    fn from_parts_with_cancel(
        sink: Box<dyn MessageSink>,
        source: Box<dyn MessageSource>,
        established: Established,
        subject: NodeId,
        shutdown: watch::Receiver<bool>,
        cancel: Option<watch::Sender<bool>>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(QUEUE_CAPACITY);
        let reader_shutdown = shutdown.clone();
        let task = tokio::spawn(run(sink, source, established, rx, shutdown));
        Self {
            tx,
            subject,
            task: Mutex::new(Some(task)),
            closed: AtomicBool::new(false),
            shutdown: reader_shutdown,
            _cancel: cancel,
        }
    }

    /// 本连接在 Owner 上被认证的节点标识。
    pub fn subject(&self) -> &NodeId {
        &self.subject
    }

    /// 关闭附件连接并等待运行任务结束（幂等）。
    pub async fn close(&self) {
        self.begin_close();
        let _ = self.tx.send(Operation::Close).await;
        let handle = self
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(handle) = handle {
            let _ = handle.await;
        }
    }

    /// 标记关闭并唤醒阻塞在本地源读取上的上传（生产路径的关闭广播由 [`OwnerLink`] 发出）。
    fn begin_close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Some(cancel) = &self._cancel {
            let _ = cancel.send_replace(true);
        }
    }

    /// `attachment.begin`：声明一次上传。该帧没有显式 ack，成功只表示已发出；受理失败会以
    /// `attachment.error` 出现在下一条回复上。
    pub async fn begin(
        &self,
        transfer: &AttachmentId,
        session: &SessionId,
        total_length: u64,
        media_type: &str,
        file_name: &str,
    ) -> Result<(), ClientError> {
        let body = FrameBody::Begin(begin_body(
            transfer,
            session,
            &self.subject,
            total_length,
            media_type,
            file_name,
        )?);
        self.send_only(body).await
    }

    /// `attachment.chunk`：发送一块并等待确认。调用方必须按 `next_offset` 顺序推进（停等）。
    pub async fn chunk(
        &self,
        transfer: &AttachmentId,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<ChunkAckBody, ClientError> {
        let body = FrameBody::Chunk(ChunkBody {
            transfer_id: uuid(transfer.as_str())?,
            offset: decimal(offset)?,
            data: ChunkData::from_bytes(data)
                .map_err(|_| ClientError::Local("an attachment chunk is not a valid size"))?,
        });
        match self.call(body, Expect::ChunkAck).await? {
            FrameBody::ChunkAck(ack) => Ok(ack),
            _ => Err(ClientError::Protocol(
                "the owner answered an attachment chunk with an unexpected frame".to_owned(),
            )),
        }
    }

    /// `attachment.seal`：提交总长度与整段摘要，等待 `attachment.sealed`。
    pub async fn seal(
        &self,
        transfer: &AttachmentId,
        length: u64,
        sha256: &Digest,
    ) -> Result<SealedAttachment, ClientError> {
        let body = FrameBody::Seal(SealBody {
            transfer_id: uuid(transfer.as_str())?,
            length: decimal(length)?,
            sha256: digest(sha256)?,
        });
        match self.call(body, Expect::Sealed).await? {
            FrameBody::Sealed(sealed) => sealed_from_wire(&sealed),
            _ => Err(ClientError::Protocol(
                "the owner answered an attachment seal with an unexpected frame".to_owned(),
            )),
        }
    }

    /// `attachment.status`：查询 Owner 上的传输状态。
    pub async fn status(&self, transfer: &AttachmentId) -> Result<TransferStatus, ClientError> {
        let body = FrameBody::Status(status_request(transfer)?);
        match self.call(body, Expect::Status).await? {
            FrameBody::Status(status) => Ok(status_from_wire(status)),
            _ => Err(ClientError::Protocol(
                "the owner answered an attachment status with an unexpected frame".to_owned(),
            )),
        }
    }

    /// `attachment.abort`：取消仍在接收的传输，返回取消后的状态。
    pub async fn abort(
        &self,
        transfer: &AttachmentId,
        reason: WireAbortReason,
    ) -> Result<TransferStatus, ClientError> {
        let body = FrameBody::Abort(AbortBody {
            transfer_id: uuid(transfer.as_str())?,
            reason,
        });
        match self.call(body, Expect::Status).await? {
            FrameBody::Status(status) => Ok(status_from_wire(status)),
            _ => Err(ClientError::Protocol(
                "the owner answered an attachment abort with an unexpected frame".to_owned(),
            )),
        }
    }

    /// `attachment.read`：读取已封存附件的一段（`max_length` 上限由 Owner 的存储判定）。
    pub async fn read(
        &self,
        attachment: &AttachmentId,
        session: &SessionId,
        offset: u64,
        max_length: u64,
    ) -> Result<ReadChunk, ClientError> {
        let body = FrameBody::Read(ReadBody {
            attachment_id: uuid(attachment.as_str())?,
            session_id: uuid(session.as_str())?,
            offset: decimal(offset)?,
            max_length: decimal(max_length)?,
        });
        match self.call(body, Expect::ReadChunk).await? {
            FrameBody::ReadChunk(chunk) => read_chunk_from_wire(&chunk),
            _ => Err(ClientError::Protocol(
                "the owner answered an attachment read with an unexpected frame".to_owned(),
            )),
        }
    }

    /// `attachment.manifest_begin`：开始登记一次提交清单（无显式 ack 帧）。
    pub async fn manifest_begin(
        &self,
        submission: &SubmissionId,
        session: &SessionId,
        attachment_count: u64,
    ) -> Result<(), ClientError> {
        let body = FrameBody::ManifestBegin(ManifestBeginBody {
            submission_id: uuid(submission.as_str())?,
            session_id: uuid(session.as_str())?,
            subject: NodeSubject::new(uuid(self.subject.as_str())?),
            attachment_count: decimal(attachment_count)?,
        });
        self.send_only(body).await
    }

    /// `attachment.manifest_page`：登记一页 `transferId`（1..=64 个，无显式 ack 帧）。
    pub async fn manifest_page(
        &self,
        submission: &SubmissionId,
        transfer_ids: &[AttachmentId],
        final_page: bool,
    ) -> Result<(), ClientError> {
        let ids = transfer_ids
            .iter()
            .map(|id| uuid(id.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty() || ids.len() > node_link_protocol::attachment::MANIFEST_PAGE_LIMIT {
            return Err(ClientError::Local(
                "a manifest page must carry 1..=64 transfer ids",
            ));
        }
        let body = FrameBody::ManifestPage(ManifestPageBody {
            submission_id: uuid(submission.as_str())?,
            transfer_ids: ids,
            final_page,
        });
        self.send_only(body).await
    }

    /// `attachment.manifest_sealed`：提交清单摘要，等待 `attachment.manifest_sealed`。
    ///
    /// 请求里的 `attachmentCount`/`sealedAt` 是接收方的回答字段；发送方按 wire 形状给出占位值
    /// （服务端只读 `submissionId`/`manifestDigest`）。
    pub async fn manifest_sealed(
        &self,
        submission: &SubmissionId,
        manifest_digest: &Digest,
    ) -> Result<SealedManifest, ClientError> {
        let body = FrameBody::ManifestSealed(ManifestSealedBody {
            submission_id: uuid(submission.as_str())?,
            attachment_count: decimal(0)?,
            manifest_digest: digest(manifest_digest)?,
            sealed_at: WireTimestamp::parse("1970-01-01T00:00:00.000Z")
                .map_err(|_| ClientError::Local("the epoch timestamp is not canonical"))?,
        });
        match self.call(body, Expect::ManifestSealed).await? {
            FrameBody::ManifestSealed(sealed) => {
                Ok(SealedManifest {
                    attachment_count: sealed.attachment_count.as_str().parse::<u64>().map_err(
                        |_| ClientError::Protocol("manifest count is not a u64".to_owned()),
                    )?,
                    manifest_digest: wire_digest(&sealed.manifest_digest)?,
                    sealed_at: core_timestamp(&sealed.sealed_at)?,
                })
            }
            _ => Err(ClientError::Protocol(
                "the owner answered a manifest seal with an unexpected frame".to_owned(),
            )),
        }
    }

    /// 流式上传：`begin` → 逐块 `chunk`（停等）→ `seal`。
    ///
    /// 只持有一个 [`CHUNK_SIZE`] 读缓冲与流式哈希：内存不随 `total_length` 增长，也不产生任何临时文件。
    /// 中途任何错误（断线、撤销、预算、摘要）都会结束上传；调用方必须显式决定是否用**同一** `transferId`
    /// 重试（Owner 对同偏移同内容幂等）。
    pub async fn upload<R: AsyncRead + Unpin>(
        &self,
        transfer: &AttachmentId,
        session: &SessionId,
        total_length: u64,
        media_type: &str,
        file_name: &str,
        mut source: R,
    ) -> Result<UploadOutcome, ClientError> {
        use sha2::Digest as _;
        self.begin(transfer, session, total_length, media_type, file_name)
            .await?;
        let mut hasher = sha2::Sha256::new();
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut offset = 0u64;
        let mut shutdown = self.shutdown.clone();
        loop {
            if *shutdown.borrow() {
                return Err(ClientError::TransferAborted);
            }
            let read = tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    let _ = changed;
                    return Err(ClientError::TransferAborted);
                }
                read = source.read(&mut buffer) => read
                    .map_err(|_| ClientError::Local("the attachment source cannot be read"))?,
            };
            if read == 0 {
                break;
            }
            let data = buffer[..read].to_vec();
            let ack = self.chunk(transfer, offset, data).await?;
            let next = ack
                .next_offset
                .as_str()
                .parse::<u64>()
                .map_err(|_| ClientError::Protocol("chunk ack is not a u64".to_owned()))?;
            let ack_len = ack
                .length
                .as_str()
                .parse::<u64>()
                .map_err(|_| ClientError::Protocol("chunk length is not a u64".to_owned()))?;
            if ack_len != read as u64 || next != offset + read as u64 {
                return Err(ClientError::Protocol(
                    "the owner acknowledged a different attachment chunk".to_owned(),
                ));
            }
            hasher.update(&buffer[..read]);
            offset += read as u64;
            if offset > total_length {
                return Err(ClientError::Local(
                    "the attachment source is longer than the declared length",
                ));
            }
        }
        if offset != total_length {
            return Err(ClientError::Local(
                "the attachment source is shorter than the declared length",
            ));
        }
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
        let sha256 = Digest::new(&encoded)
            .map_err(|_| ClientError::Local("the attachment digest cannot be encoded"))?;
        let sealed = self.seal(transfer, offset, &sha256).await?;
        Ok(UploadOutcome {
            sealed,
            sent: offset,
        })
    }

    /// 把一帧交给运行任务并等待匹配的回复。
    async fn call(&self, body: FrameBody, expect: Expect) -> Result<FrameBody, ClientError> {
        let reply = self.dispatch(body, Some(expect)).await?;
        reply.ok_or_else(|| {
            ClientError::Protocol("the owner did not answer the attachment request".to_owned())
        })
    }

    /// 把无回复的一帧交给运行任务。
    async fn send_only(&self, body: FrameBody) -> Result<(), ClientError> {
        self.dispatch(body, None).await.map(|_| ())
    }

    async fn dispatch(
        &self,
        body: FrameBody,
        expect: Option<Expect>,
    ) -> Result<Option<FrameBody>, ClientError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ClientError::Closed);
        }
        let (reply, reply_rx) = oneshot::channel();
        self.tx
            .send(Operation::Frame {
                body,
                expect,
                reply,
            })
            .await
            .map_err(|_| ClientError::Closed)?;
        reply_rx.await.map_err(|_| ClientError::Closed)?
    }
}

impl Drop for AttachmentForwarder {
    fn drop(&mut self) {
        // 不能在这里等待（可能在 runtime 之外）；只尽力让运行任务与调用方看到关闭。
        self.begin_close();
        let _ = self.tx.try_send(Operation::Close);
    }
}

/// 运行任务：独占读写两半，串行处理请求与入站控制帧。
async fn run(
    mut sink: Box<dyn MessageSink>,
    mut source: Box<dyn MessageSource>,
    established: Established,
    mut queue: mpsc::Receiver<Operation>,
    mut shutdown: watch::Receiver<bool>,
) {
    let connection_id = established.connection_id.clone();
    let mut next_sequence = established.next_sequence;
    // 握手已经消费了 Owner 的认证后序号 1（`node.ready`），第一条附件回复应是 2。
    let mut inbound_next = established.inbound_next;
    let mut pending: PendingReply = None;
    loop {
        if *shutdown.borrow() {
            // `revoke`/`shutdown`：立即中止在途请求，不等对端先断开。
            fail_pending(&mut pending, ClientError::TransferAborted);
            let _ = sink.close().await;
            return;
        }
        if let Some((expect, _)) = pending.as_ref() {
            // 有请求在途：只处理入站消息，直到拿到匹配回复或失败。
            let expect = *expect;
            let outcome = await_reply(
                &mut sink,
                &mut source,
                &connection_id,
                &mut inbound_next,
                &mut next_sequence,
                expect,
                &mut shutdown,
            )
            .await;
            let (_, reply) = pending
                .take()
                .expect("the pending slot is occupied when the loop enters this branch");
            match outcome {
                Ok(body) => {
                    let _ = reply.send(Ok(Some(body)));
                }
                Err(error) => {
                    let _ = reply.send(Err(error));
                    // 错误后连接状态不可判定：关闭并结束任务（在途请求已收到同一错误）。
                    let _ = sink.close().await;
                    return;
                }
            }
            continue;
        }
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                let _ = changed;
                fail_pending(&mut pending, ClientError::TransferAborted);
                let _ = sink.close().await;
                return;
            }
            request = queue.recv() => {
                match request {
                    None | Some(Operation::Close) => {
                        let _ = sink.close().await;
                        return;
                    }
                    Some(Operation::Frame { body, expect, reply }) => {
                        let text = match encode_frame(&body, &connection_id, next_sequence) {
                            Ok(text) => text,
                            Err(error) => {
                                let _ = reply.send(Err(error));
                                continue;
                            }
                        };
                        next_sequence += 1;
                        if sink.send(text).await.is_err() {
                            let _ = reply.send(Err(ClientError::Closed));
                            return;
                        }
                        match expect {
                            None => {
                                let _ = reply.send(Ok(None));
                            }
                            Some(expect) => pending = Some((expect, reply)),
                        }
                    }
                }
            }
            incoming = source.recv() => {
                // 空闲时也要处理心跳、迟到错误与对端关闭。
                match incoming {
                    Ok(Incoming::Text(text)) => {
                        if let Err(error) = handle_idle(
                            &mut sink,
                            &text,
                            &connection_id,
                            &mut inbound_next,
                            &mut next_sequence,
                        ).await {
                            tracing::debug!(
                                event = "node_link_client.attachment_idle_frame",
                                error = %error,
                                "the attachment connection received an unusable frame while idle"
                            );
                            if matches!(error, ClientError::Revoked | ClientError::Closed) {
                                let _ = sink.close().await;
                                return;
                            }
                        }
                    }
                    Ok(Incoming::Closed(_)) | Err(_) => {
                        let _ = sink.close().await;
                        return;
                    }
                }
            }
        }
    }
}

/// 把在途请求以显式错误结束（取消/关闭路径）。
fn fail_pending(pending: &mut PendingReply, error: ClientError) {
    if let Some((_, reply)) = pending.take() {
        let _ = reply.send(Err(error));
    }
}

/// 等待与 `expect` 匹配的回复；同时处理心跳、错误帧与关闭信号。
async fn await_reply(
    sink: &mut Box<dyn MessageSink>,
    source: &mut Box<dyn MessageSource>,
    connection_id: &Uuid,
    inbound_next: &mut u64,
    next_sequence: &mut u64,
    expect: Expect,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<FrameBody, ClientError> {
    let deadline = tokio::time::Instant::now() + REPLY_TIMEOUT;
    loop {
        let incoming = tokio::select! {
            biased;
            _ = shutdown.changed() => return Err(ClientError::TransferAborted),
            result = tokio::time::timeout_at(deadline, source.recv()) => {
                match result {
                    Ok(Ok(incoming)) => incoming,
                    Ok(Err(_)) => return Err(ClientError::Closed),
                    Err(_) => return Err(ClientError::Timeout),
                }
            }
        };
        match incoming {
            Incoming::Closed(Some(4410)) => return Err(ClientError::Revoked),
            Incoming::Closed(_) => return Err(ClientError::Closed),
            Incoming::Text(text) => match decode_incoming(&text, connection_id, inbound_next) {
                Ok(IncomingFrame::Attachment(frame)) => {
                    if let FrameBody::Error(error) = &frame.body {
                        return Err(wire_error(error));
                    }
                    if matches_expect(expect, &frame.body) {
                        return Ok(frame.body);
                    }
                    return Err(ClientError::Protocol(
                        "the owner sent an unexpected attachment frame".to_owned(),
                    ));
                }
                Ok(IncomingFrame::Control(envelope)) => {
                    handle_control(sink, &envelope, next_sequence).await?;
                }
                Err(error) => return Err(error),
            },
        }
    }
}

/// 空闲状态下的入站帧：只允许控制帧；附件帧是协议错误。
async fn handle_idle(
    sink: &mut Box<dyn MessageSink>,
    text: &str,
    connection_id: &Uuid,
    inbound_next: &mut u64,
    next_sequence: &mut u64,
) -> Result<(), ClientError> {
    match decode_incoming(text, connection_id, inbound_next) {
        Ok(IncomingFrame::Control(envelope)) => {
            handle_control(sink, &envelope, next_sequence).await
        }
        Ok(IncomingFrame::Attachment(frame)) => match frame.body {
            FrameBody::Error(error) => Err(wire_error(&error)),
            _ => Err(ClientError::Protocol(
                "the owner sent an attachment frame with no request in flight".to_owned(),
            )),
        },
        Err(error) => Err(error),
    }
}

enum IncomingFrame {
    Attachment(Frame),
    Control(Envelope),
}

/// 解码一条入站文本：附件帧优先；控制帧走业务信封（同一套连接字段与序号）。
fn decode_incoming(
    text: &str,
    connection_id: &Uuid,
    inbound_next: &mut u64,
) -> Result<IncomingFrame, ClientError> {
    match Frame::decode(text) {
        Ok(frame) => {
            if frame.connection_id.as_str() != connection_id.as_str() {
                return Err(ClientError::Protocol(
                    "the attachment frame names a different connection".to_owned(),
                ));
            }
            check_sequence(frame.connection_sequence.as_str(), inbound_next)?;
            Ok(IncomingFrame::Attachment(frame))
        }
        Err(node_link_protocol::attachment::DecodeError::TypeUnsupported)
        | Err(node_link_protocol::attachment::DecodeError::InvalidJson) => {
            let envelope = Envelope::decode(text).map_err(|_| {
                ClientError::Protocol("the frame is not a known envelope".to_owned())
            })?;
            let Some(connection) = envelope.connection() else {
                return Err(ClientError::Protocol(
                    "a post-auth control frame must carry connection fields".to_owned(),
                ));
            };
            if connection.connection_id.as_str() != connection_id.as_str() {
                return Err(ClientError::Protocol(
                    "the control frame names a different connection".to_owned(),
                ));
            }
            check_sequence(connection.connection_sequence.as_str(), inbound_next)?;
            Ok(IncomingFrame::Control(envelope))
        }
        Err(error) => Err(ClientError::Protocol(format!(
            "the attachment frame cannot be decoded: {error:?}"
        ))),
    }
}

/// 入站序号必须正好是下一个期望值（两个方向各自从 1 开始，严格加一）。
fn check_sequence(text: &str, inbound_next: &mut u64) -> Result<(), ClientError> {
    let sequence = text
        .parse::<u64>()
        .map_err(|_| ClientError::Protocol("sequence is not a u64".to_owned()))?;
    if sequence != *inbound_next {
        return Err(ClientError::Protocol(
            "the inbound connection sequence is not the next expected value".to_owned(),
        ));
    }
    *inbound_next += 1;
    Ok(())
}

/// 控制帧：心跳原样回填、错误映射、撤销显式失败。
async fn handle_control(
    sink: &mut Box<dyn MessageSink>,
    envelope: &Envelope,
    next_sequence: &mut u64,
) -> Result<(), ClientError> {
    match envelope.message_type() {
        MessageType::LinkPing => {
            let ping: Ping = serde_json::from_str(envelope.body().get())
                .map_err(|_| ClientError::Protocol("the ping body is malformed".to_owned()))?;
            let pong = Ping { nonce: ping.nonce };
            let body = serde_json::to_string(&pong)
                .map_err(|_| ClientError::Local("the pong body cannot be encoded"))?;
            let raw = serde_json::value::RawValue::from_string(body)
                .map_err(|_| ClientError::Local("the pong body cannot be encoded"))?;
            let message_id = uuid(&uuid::Uuid::new_v4().to_string())?;
            let connection = wire_connection_fields(
                &envelope
                    .connection()
                    .expect("post-auth envelopes carry connection fields")
                    .connection_id,
                *next_sequence,
            )?;
            let pong = Envelope::new(MessageType::LinkPong, message_id, Some(connection), raw)
                .map_err(|_| ClientError::Local("the pong envelope cannot be assembled"))?;
            let text = pong
                .encode()
                .map_err(|_| ClientError::Local("the pong envelope cannot be encoded"))?;
            *next_sequence += 1;
            sink.send(text).await.map_err(|_| ClientError::Closed)?;
            Ok(())
        }
        MessageType::LinkError => {
            let error: node_link_protocol::error::Body =
                serde_json::from_str(envelope.body().get())
                    .map_err(|_| ClientError::Protocol("the error body is malformed".to_owned()))?;
            Err(wire_error_code(error.code, error.message.as_str()))
        }
        MessageType::NodeTrustRevoked => Err(ClientError::Revoked),
        _ => Err(ClientError::Protocol(
            "the owner sent a control frame that is not allowed on an attachment connection"
                .to_owned(),
        )),
    }
}

fn matches_expect(expect: Expect, body: &FrameBody) -> bool {
    matches!(
        (expect, body),
        (Expect::ChunkAck, FrameBody::ChunkAck(_))
            | (Expect::Sealed, FrameBody::Sealed(_))
            | (Expect::Status, FrameBody::Status(_))
            | (Expect::ReadChunk, FrameBody::ReadChunk(_))
            | (Expect::ManifestSealed, FrameBody::ManifestSealed(_))
    )
}

fn encode_frame(
    body: &FrameBody,
    connection_id: &Uuid,
    sequence: u64,
) -> Result<String, ClientError> {
    let message_id = uuid(&uuid::Uuid::new_v4().to_string())?;
    Frame::new(
        message_id,
        connection_id.clone(),
        decimal(sequence)?,
        body.clone(),
    )
    .encode()
    .map_err(|_| ClientError::Local("an attachment frame cannot be encoded"))
}

fn begin_body(
    transfer: &AttachmentId,
    session: &SessionId,
    subject: &NodeId,
    total_length: u64,
    media_type: &str,
    file_name: &str,
) -> Result<BeginBody, ClientError> {
    Ok(BeginBody {
        transfer_id: uuid(transfer.as_str())?,
        session_id: uuid(session.as_str())?,
        subject: NodeSubject::new(uuid(subject.as_str())?),
        total_length: decimal(total_length)?,
        chunk_size: node_link_protocol::attachment::FixedChunkSize::new(CHUNK_SIZE as u64)
            .map_err(|_| ClientError::Local("the fixed chunk size is not representable"))?,
        media_type: NonEmptyText::parse(media_type)
            .map_err(|_| ClientError::Local("the media type is not a non-empty text"))?,
        file_name: NonEmptyText::parse(file_name)
            .map_err(|_| ClientError::Local("the file name is not a non-empty text"))?,
    })
}

fn status_request(transfer: &AttachmentId) -> Result<StatusBody, ClientError> {
    Ok(StatusBody {
        transfer_id: uuid(transfer.as_str())?,
        state: TransferState::Receiving,
        received_length: decimal(0)?,
        next_offset: decimal(0)?,
        sealed_sha256: Nullable::null(),
    })
}

fn read_chunk_from_wire(chunk: &ReadChunkBody) -> Result<ReadChunk, ClientError> {
    Ok(ReadChunk {
        offset: decimal_value(&chunk.offset, "read offset")?,
        length: decimal_value(&chunk.length, "read length")?,
        total_length: decimal_value(&chunk.total_length, "read total")?,
        data: chunk.data.as_bytes().to_vec(),
        eof: chunk.eof,
    })
}

fn status_from_wire(body: StatusBody) -> TransferStatus {
    TransferStatus {
        state: match body.state {
            TransferState::Receiving => AttachmentTransferState::Receiving,
            TransferState::Sealed => AttachmentTransferState::Sealed,
            TransferState::Aborted => AttachmentTransferState::Aborted,
            TransferState::Expired => AttachmentTransferState::Expired,
        },
        received_length: decimal_value(&body.received_length, "received length").unwrap_or(0),
        next_offset: decimal_value(&body.next_offset, "next offset").unwrap_or(0),
        sealed_sha256: body
            .sealed_sha256
            .as_ref()
            .and_then(|value| wire_digest(value).ok()),
    }
}

fn sealed_from_wire(body: &SealedBody) -> Result<SealedAttachment, ClientError> {
    Ok(SealedAttachment {
        length: decimal_value(&body.length, "sealed length")?,
        sha256: wire_digest(&body.sha256)?,
        sealed_at: core_timestamp(&body.sealed_at)?,
    })
}

fn wire_error(body: &ErrorBody) -> ClientError {
    wire_error_code(body.code, body.message.as_str())
}

fn wire_error_code(code: ErrorCode, message: &str) -> ClientError {
    match code {
        ErrorCode::AuthNodeRevoked | ErrorCode::AttachmentRevoked => ClientError::Revoked,
        ErrorCode::AttachmentNegotiationRequired => ClientError::Unsupported,
        ErrorCode::AttachmentTransferNotFound => ClientError::Transfer("transfer_not_found"),
        ErrorCode::AttachmentBindingMismatch => ClientError::Transfer("binding_mismatch"),
        ErrorCode::AttachmentOffsetMismatch => ClientError::Transfer("offset_mismatch"),
        ErrorCode::AttachmentChunkConflict => ClientError::Transfer("chunk_conflict"),
        ErrorCode::AttachmentDigestMismatch => ClientError::Transfer("digest_mismatch"),
        ErrorCode::AttachmentSealed => ClientError::Transfer("already_sealed"),
        ErrorCode::AttachmentBudgetExhausted => ClientError::Transfer("budget_exhausted"),
        ErrorCode::AttachmentExpired => ClientError::Transfer("expired"),
        _ => ClientError::Protocol(format!("{}: {message}", code.as_str())),
    }
}

fn uuid(text: &str) -> Result<Uuid, ClientError> {
    Uuid::parse(text).map_err(|_| ClientError::Local("a local identifier is not a canonical uuid"))
}

fn decimal(value: u64) -> Result<DecimalString, ClientError> {
    DecimalString::parse(&value.to_string())
        .map_err(|_| ClientError::Local("a decimal value cannot be encoded"))
}

fn decimal_value(value: &DecimalString, what: &'static str) -> Result<u64, ClientError> {
    value
        .as_str()
        .parse::<u64>()
        .map_err(|_| ClientError::Protocol(format!("the attachment {what} is not a u64")))
}

fn digest(value: &Digest) -> Result<Base64Url<32>, ClientError> {
    Base64Url::<32>::parse(value.as_str())
        .map_err(|_| ClientError::Local("the digest is not a canonical base64url value"))
}

fn wire_digest(value: &Base64Url<32>) -> Result<Digest, ClientError> {
    use base64::Engine as _;
    let text = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.as_bytes());
    Digest::new(&text).map_err(|_| ClientError::Local("the digest cannot be encoded"))
}

fn core_timestamp(value: &WireTimestamp) -> Result<Timestamp, ClientError> {
    Timestamp::new(value.as_str())
        .map_err(|_| ClientError::Protocol("the timestamp is not canonical".to_owned()))
}
