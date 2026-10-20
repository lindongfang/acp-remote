//! 独立附件连接的业务引擎（`docs/SYNC_PROTOCOL.md` §3.4、`docs/NODE_LINK_PROTOCOL.md` §12.9）。
//!
//! 本模块**不含 wire 类型**：Sync 与 Node Link 的帧 DTO、错误码与主体形状各自定义在两个入站适配器里
//! （`server::sync::attachments`、`server::node_link::attachments`），它们把各自解码出的帧折成
//! [`Request`]，把 [`Reply`]/[`Fault`] 编回各自协议的帧与错误体。这样做的理由有两条：
//!
//! - 两个协议不复用对方的 wire DTO（`AGENTS.md` §6、`MODULE_ARCHITECTURE.md` §4.3/§4.4）；
//! - 分块、封存、清单与预算的判定只有一份实现。两份拷贝在「重复块」「跨会话」「预算」这类边界上迟早
//!   漂移，而它们直接决定正文是否会以错误长度或错误摘要被封存。
//!
//! 依赖纪律：只使用 `core::ports`/`core::model` 的值对象与端口（[`AttachmentTransferStore`]、
//! [`Clock`]），不 import 任何协议 crate、不查 SQLite、不调用平级适配器。附件正文由端口落盘，本模块
//! 只搬运单块（≤ 32 KiB），内存工作集与文件大小无关。
//!
//! ## 三个进程内状态
//!
//! - **活动传输计数**（单设备 2 / 单节点 8）：端口没有「列出某主体的活动传输」的读面，因此计数只能由
//!   本进程维护。它按主体跨连接累计（同一条设备的业务连接与附件连接共享同一个计数器），在封存/取消/
//!   失效时释放，并在**预留它的附件连接关闭**时释放（[`Engine::release_connection`]）：否则客户端开两个
//!   上传后断线，这两个登记就永久占着预算，之后该主体的每次 `begin` 都是 `budget_exhausted`——一次可
//!   恢复的客户端放弃会变成对该主体的功能锁定。daemon 重启后归零，持久化的 24 小时清理与预算判定因此
//!   是两件事（前者由 `AttachmentTransferStore::prune_transfers` 负责）。
//! - **清单草稿**：`manifest_page` 只给 `transferId`，而端口的 `SubmissionPage` 要的是**已封存**的
//!   `AttachmentId`，两者之间的顺序与映射必须由本层记住。草稿按 `submissionId` 保存；断线重连后客户端
//!   重放 `manifest_begin` + 各页即可重建（页登记对同位置同内容幂等），因此草稿不需要持久化。
//! - **每连接帧预算**：附件族独立计数（§14），不占用业务命令的 120 次/分钟。单连接 60 秒内最多
//!   [`FRAMES_PER_MINUTE`] 帧；超出回 `budget_exhausted`（retryable），**不**关闭连接——附件是批量
//!   数据，关连接会让已经传完的块全部白费，而错误本身已经给了退避信号。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use acp_core::model::{
    ATTACHMENT_CHUNK_SIZE, AttachmentId, AttachmentSubject, AttachmentTransfer,
    AttachmentTransferState, DeviceId, Digest, NodeId, PortError, SessionId, SubmissionId,
    Timestamp, UnavailableKind,
};
use acp_core::ports::{
    AttachmentRead, AttachmentTransferStore, Clock, SubmissionBegin, SubmissionPage,
    SubmissionSeal, TransferBegin, TransferChunk, TransferSeal,
};

/// 单连接附件帧预算（60 秒固定窗口）。
///
/// 取值理由：单块上限 32 KiB，100 帧/秒 ≒ 3.1 MiB/s 的裸吞吐，足够本机与局域网的附件上传；同时把
/// 一条连接的最坏 CPU/磁盘压力钉在可预期的量级上。它是**运行保护**，不是产品额度（总量只受磁盘与
/// 会话保留策略约束），也不进入 `auth.authenticated.limits`。
pub const FRAMES_PER_MINUTE: u32 = 6_000;

/// 单设备活动传输上限（`transportFrames.budget.maxActiveTransfersPerDevice`）。
pub const MAX_ACTIVE_TRANSFERS_PER_DEVICE: usize = 2;

/// 单节点活动传输上限（`transportFrames.budget.maxActiveTransfersPerNode`）。
pub const MAX_ACTIVE_TRANSFERS_PER_NODE: usize = 8;

/// 帧预算窗口。
const FRAME_WINDOW: Duration = Duration::from_secs(60);

/// 预算表里登记的单块大小（编译期与 core 常量对齐）。
const CHUNK_SIZE: usize = ATTACHMENT_CHUNK_SIZE as usize;

/// 一次请求的主体（已由适配器从**已认证身份**构造，不取自帧里自报的字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// Sync：设备。
    Device(String),
    /// Node Link：Access 节点。
    Node(String),
}

impl Subject {
    /// 与 core 的附件主体逐字比较（帧里的 `subject` 只是重复声明，权威是本连接的身份）。
    fn matches(&self, subject: &AttachmentSubject) -> bool {
        match (self, subject) {
            (Subject::Device(id), AttachmentSubject::Device(device)) => id == device.as_str(),
            (Subject::Node(id), AttachmentSubject::Node { access_node }) => {
                id == access_node.as_str()
            }
            _ => false,
        }
    }

    /// 预算键：设备与节点各自计数（同一 UUID 在两侧不会混用）。
    fn key(&self) -> String {
        match self {
            Subject::Device(id) => format!("device:{id}"),
            Subject::Node(id) => format!("node:{id}"),
        }
    }

    fn limit(&self) -> usize {
        match self {
            Subject::Device(_) => MAX_ACTIVE_TRANSFERS_PER_DEVICE,
            Subject::Node(_) => MAX_ACTIVE_TRANSFERS_PER_NODE,
        }
    }

    /// core 的附件主体（端口签名要的那一份）。
    pub fn core(&self) -> Option<AttachmentSubject> {
        match self {
            Subject::Device(id) => DeviceId::parse(id).ok().map(AttachmentSubject::Device),
            Subject::Node(id) => NodeId::parse(id)
                .ok()
                .map(|access_node| AttachmentSubject::Node { access_node }),
        }
    }
}

/// 一条已解码的附件请求（协议中立）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// `attachment.begin`。
    Begin(BeginRequest),
    /// `attachment.chunk`。
    Chunk {
        transfer: AttachmentId,
        offset: u64,
        data: Vec<u8>,
    },
    /// `attachment.seal`。
    Seal {
        transfer: AttachmentId,
        length: u64,
        sha256: Digest,
    },
    /// `attachment.status`。
    Status { transfer: AttachmentId },
    /// `attachment.abort`。
    Abort {
        transfer: AttachmentId,
        reason: AbortReason,
    },
    /// `attachment.read`。
    Read {
        attachment: AttachmentId,
        session: SessionId,
        offset: u64,
        max_length: u64,
    },
    /// `attachment.manifest_begin`。
    ManifestBegin {
        submission: SubmissionId,
        session: SessionId,
        subject: Subject,
        attachment_count: u64,
    },
    /// `attachment.manifest_page`。
    ManifestPage {
        submission: SubmissionId,
        transfer_ids: Vec<AttachmentId>,
        final_page: bool,
    },
    /// `attachment.manifest_sealed`。
    ManifestSealed {
        submission: SubmissionId,
        manifest_digest: Digest,
    },
}

/// `attachment.begin` 的字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginRequest {
    pub transfer: AttachmentId,
    pub session: SessionId,
    pub subject: Subject,
    pub total_length: u64,
    pub chunk_size: u64,
    pub media_type: String,
    pub file_name: String,
}

/// `attachment.abort.reason` 的封闭词表（协议中立）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortReason {
    ClientCancelled,
    BudgetExceeded,
    Revoked,
    DigestMismatch,
}

/// 引擎的成功回复（`None` = 该请求不产生回复，例如 `begin`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    ChunkAck {
        transfer: AttachmentId,
        offset: u64,
        length: u64,
        duplicate: bool,
        next_offset: u64,
    },
    Sealed {
        transfer: AttachmentId,
        attachment: AttachmentId,
        length: u64,
        sha256: Digest,
        sealed_at: Timestamp,
    },
    Status(TransferStatus),
    ReadChunk {
        attachment: AttachmentId,
        session: SessionId,
        offset: u64,
        length: u64,
        total_length: u64,
        data: Vec<u8>,
        eof: bool,
    },
    ManifestSealed {
        submission: SubmissionId,
        attachment_count: u64,
        manifest_digest: Digest,
        sealed_at: Timestamp,
    },
}

/// `attachment.status` 的投影（状态、已接收长度与封存摘要）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferStatus {
    pub transfer: AttachmentId,
    pub state: AttachmentTransferState,
    pub received_length: u64,
    pub next_offset: u64,
    pub sealed_sha256: Option<Digest>,
}

/// 失败的协议中立分类（适配器映射为各自错误码）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// 未协商附件 feature（适配器在进入引擎**之前**判定，保留给接线错误）。
    NegotiationRequired,
    /// 传输或清单不存在（含已取消/已过期）。
    TransferNotFound,
    /// 会话、主体或标识归属不符（不泄露内容）。
    BindingMismatch,
    /// 偏移与期望不符（可重试，`details` 给出期望与实际）。
    OffsetMismatch { expected: u64, actual: u64 },
    /// 同偏移不同内容。
    ChunkConflict { offset: u64 },
    /// 长度或摘要不符。
    DigestMismatch,
    /// 传输已封存（不能继续写入），或清单已封存。
    Sealed,
    /// 活动传输或帧预算耗尽（可重试）。
    BudgetExhausted,
    /// 请求在语法上不可接受（`protocol.schema_invalid` 一类）。
    Invalid,
    /// 端口/磁盘故障。
    Unavailable,
}

impl Fault {
    /// 该失败是否可重试（registry 的 `retryable` 语义）。
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Fault::OffsetMismatch { .. } | Fault::BudgetExhausted | Fault::Unavailable
        )
    }
}

/// 附件引擎（进程内唯一；两条入站适配器各持一个句柄，但共享同一个实例的语义由组合根决定）。
pub struct Engine {
    transfers: Arc<dyn AttachmentTransferStore>,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("attachment::Engine")
            .field("active", &self.active_total())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct State {
    /// 主体键 → 仍在 `receiving` 的传输 → **预留它的附件连接**。
    ///
    /// 记连接是为了在断线时只释放该连接自己预留的槽位：同一主体的另一条连接上仍在进行的传输不能被
    /// 别人的关闭顺手清掉（那会让预算失效，多开连接即可绕过单设备 2 / 单节点 8 的上限）。
    receiving: BTreeMap<String, BTreeMap<String, String>>,
    /// 清单草稿（见模块文档；不持久化）。
    manifests: BTreeMap<String, ManifestDraft>,
    /// 每连接的固定窗口帧预算。
    windows: BTreeMap<String, Window>,
}

#[derive(Debug)]
struct ManifestDraft {
    session: SessionId,
    subject: Subject,
    declared: u64,
    /// 按页登记顺序累积的 `transferId`（清单顺序）。
    transfer_ids: Vec<AttachmentId>,
    final_seen: bool,
}

#[derive(Debug)]
struct Window {
    started: Instant,
    count: u32,
}

impl Engine {
    /// 装配。`transfers` 与 `clock` 由组合根注入（`UseCases::attachment_transfers`/`UseCases::clock`）。
    pub fn new(transfers: Arc<dyn AttachmentTransferStore>, clock: Arc<dyn Clock>) -> Arc<Self> {
        Arc::new(Self {
            transfers,
            clock,
            state: Mutex::new(State::default()),
        })
    }

    /// 当前进程里登记的活动传输数（诊断与单测）。
    pub fn active_total(&self) -> usize {
        lock(&self.state)
            .receiving
            .values()
            .map(BTreeMap::len)
            .sum()
    }

    /// 处理一条请求。
    ///
    /// `connection` 是本次附件连接的 `connectionId`（帧预算键）；`subject` 来自已认证身份。
    pub async fn handle(
        &self,
        connection: &str,
        subject: &Subject,
        request: Request,
    ) -> Result<Option<Reply>, Fault> {
        self.admit_frame(connection)?;
        let at = Timestamp::new(self.clock.now().as_str()).map_err(|_| Fault::Unavailable)?;
        match request {
            Request::Begin(begin) => self.begin(connection, subject, begin, at).await,
            Request::Chunk {
                transfer,
                offset,
                data,
            } => self.chunk(subject, transfer, offset, data, at).await,
            Request::Seal {
                transfer,
                length,
                sha256,
            } => self.seal(subject, transfer, length, sha256, at).await,
            Request::Status { transfer } => self.status(subject, &transfer).await,
            Request::Abort { transfer, reason } => self.abort(subject, &transfer, reason, at).await,
            Request::Read {
                attachment,
                session,
                offset,
                max_length,
            } => {
                self.read(subject, attachment, session, offset, max_length)
                    .await
            }
            Request::ManifestBegin {
                submission,
                session,
                subject: declared,
                attachment_count,
            } => {
                self.manifest_begin(
                    subject,
                    submission,
                    session,
                    &declared,
                    attachment_count,
                    at,
                )
                .await
            }
            Request::ManifestPage {
                submission,
                transfer_ids,
                final_page,
            } => {
                self.manifest_page(subject, submission, transfer_ids, final_page)
                    .await
            }
            Request::ManifestSealed {
                submission,
                manifest_digest,
            } => {
                self.manifest_sealed(subject, submission, manifest_digest, at)
                    .await
            }
        }
    }

    /// 每连接固定窗口预算。窗口按连接记录；超过 1,024 个连接的旧窗口在新建时淘汰。
    fn admit_frame(&self, connection: &str) -> Result<(), Fault> {
        let now = Instant::now();
        let mut state = lock(&self.state);
        if !state.windows.contains_key(connection) && state.windows.len() >= 1_024 {
            state
                .windows
                .retain(|_, window| now.duration_since(window.started) < FRAME_WINDOW);
        }
        let window = state
            .windows
            .entry(connection.to_owned())
            .or_insert(Window {
                started: now,
                count: 0,
            });
        if now.duration_since(window.started) >= FRAME_WINDOW {
            window.started = now;
            window.count = 0;
        }
        if window.count >= FRAMES_PER_MINUTE {
            return Err(Fault::BudgetExhausted);
        }
        window.count += 1;
        Ok(())
    }

    async fn begin(
        &self,
        connection: &str,
        subject: &Subject,
        begin: BeginRequest,
        at: Timestamp,
    ) -> Result<Option<Reply>, Fault> {
        if begin.chunk_size != CHUNK_SIZE as u64 {
            // `chunkSize` 是 `const 32768`（schema），不是可协商值。
            return Err(Fault::Invalid);
        }
        if begin.subject != *subject {
            return Err(Fault::BindingMismatch);
        }
        let core_subject = subject.core().ok_or(Fault::BindingMismatch)?;
        // 预算检查与槽位预留必须**在同一个临界区**内完成：先读计数、放开锁、再 `await` 写存储、成功后才
        // 登记，是典型的 check-then-act——同一主体的并发 `begin` 会全部看到旧计数并通过检查，随后全部
        // 落库，突破单设备 2 / 单节点 8 的上限。
        let reserved = self.reserve(connection, subject, &begin.transfer)?;
        let release = |fault: Fault| {
            if reserved {
                self.release(connection, subject, &begin.transfer);
            }
            fault
        };
        let session = begin.session.clone();
        let write = TransferBegin {
            transfer: begin.transfer.clone(),
            session: begin.session,
            subject: core_subject,
            total_length: begin.total_length,
            media_type: begin.media_type,
            file_name: begin.file_name,
            at,
        };
        match self.transfers.begin_transfer(write).await {
            Ok(_) => Ok(None),
            // 重复 `begin`：同参数是幂等重放（不重复计数、不报错），不同参数是标识复用。
            Err(PortError::Conflict(_)) => {
                let existing = match self.transfers.transfer(&begin.transfer).await {
                    Ok(existing) => existing,
                    Err(_) => return Err(release(Fault::Unavailable)),
                };
                let existing = existing.ok_or_else(|| release(Fault::TransferNotFound))?;
                if existing.session == session && subject.matches(&existing.subject) {
                    Ok(None)
                } else {
                    Err(release(Fault::BindingMismatch))
                }
            }
            Err(error) => Err(release(port_fault(&error))),
        }
    }

    /// 在同一临界区内检查主体预算并预留槽位（`transferId` 已登记时返回 `false`，不重复计数）。
    fn reserve(
        &self,
        connection: &str,
        subject: &Subject,
        transfer: &AttachmentId,
    ) -> Result<bool, Fault> {
        let mut state = lock(&self.state);
        let registered = state.receiving.entry(subject.key()).or_default();
        if registered.contains_key(transfer.as_str()) {
            return Ok(false);
        }
        if registered.len() >= subject.limit() {
            return Err(Fault::BudgetExhausted);
        }
        registered.insert(transfer.as_str().to_owned(), connection.to_owned());
        Ok(true)
    }

    /// 撤回本连接的预留（只移除本连接自己预留的那个槽位）。
    fn release(&self, connection: &str, subject: &Subject, transfer: &AttachmentId) {
        let mut state = lock(&self.state);
        let empty = {
            let Some(registered) = state.receiving.get_mut(&subject.key()) else {
                return;
            };
            if registered
                .get(transfer.as_str())
                .is_some_and(|owner| owner == connection)
            {
                registered.remove(transfer.as_str());
            }
            registered.is_empty()
        };
        if empty {
            state.receiving.remove(&subject.key());
        }
    }

    /// 附件连接关闭：释放该连接预留的全部活动传输槽位与帧预算窗口。
    ///
    /// 只按 `connectionId` 释放：客户端放弃上传（断线、崩溃或主动关闭）后，登记若继续留在 `receiving`
    /// 就会永久占住该主体的预算。持久化的半成品传输仍由 `prune_transfers` 在 24 小时后清理；这里只清
    /// 进程内的预算账。客户端重连后用同一个 `transferId` 重新 `begin` 即可继续（幂等重放）。
    pub fn release_connection(&self, connection: &str) {
        let mut state = lock(&self.state);
        state.receiving.retain(|_, registered| {
            registered.retain(|_, owner| owner != connection);
            !registered.is_empty()
        });
        state.windows.remove(connection);
    }

    async fn chunk(
        &self,
        subject: &Subject,
        transfer: AttachmentId,
        offset: u64,
        data: Vec<u8>,
        at: Timestamp,
    ) -> Result<Option<Reply>, Fault> {
        if data.is_empty() || data.len() > CHUNK_SIZE {
            return Err(Fault::Invalid);
        }
        let current = self.load(subject, &transfer).await?;
        if current.state != AttachmentTransferState::Receiving {
            self.forget(subject, &transfer);
            return Err(match current.state {
                AttachmentTransferState::Sealed => Fault::Sealed,
                _ => Fault::TransferNotFound,
            });
        }
        let expected = current.received_length;
        let length = data.len() as u64;
        let outcome = self
            .transfers
            .append_chunk(TransferChunk {
                transfer: transfer.clone(),
                offset,
                data,
                at,
            })
            .await
            .map_err(|error| match &error {
                PortError::InvalidRequest(message)
                    if *message == "chunk offset does not match the next expected offset"
                        || *message == "chunk would exceed the declared total length" =>
                {
                    Fault::OffsetMismatch {
                        expected,
                        actual: offset,
                    }
                }
                PortError::InvalidRequest("transfer is not receiving") => Fault::Sealed,
                PortError::Conflict(_) => Fault::ChunkConflict { offset },
                PortError::NotFound(_) => Fault::TransferNotFound,
                other => port_fault(other),
            })?;
        Ok(Some(Reply::ChunkAck {
            transfer,
            offset,
            length,
            duplicate: outcome.duplicate,
            next_offset: outcome.next_offset,
        }))
    }

    async fn seal(
        &self,
        subject: &Subject,
        transfer: AttachmentId,
        length: u64,
        sha256: Digest,
        at: Timestamp,
    ) -> Result<Option<Reply>, Fault> {
        let current = self.load(subject, &transfer).await?;
        if current.state == AttachmentTransferState::Aborted
            || current.state == AttachmentTransferState::Expired
        {
            self.forget(subject, &transfer);
            return Err(Fault::TransferNotFound);
        }
        let sealed = self
            .transfers
            .seal_transfer(TransferSeal {
                transfer: transfer.clone(),
                length,
                sha256,
                at,
            })
            .await
            .map_err(|error| match &error {
                PortError::InvalidRequest(message)
                    if *message == "sealed length does not match the received bytes"
                        || *message == "attachment digest does not match the streamed bytes" =>
                {
                    Fault::DigestMismatch
                }
                PortError::InvalidRequest("transfer is not receiving") => Fault::Sealed,
                PortError::Conflict(_) => Fault::Sealed,
                PortError::NotFound(_) => Fault::TransferNotFound,
                other => port_fault(other),
            })?;
        self.forget(subject, &transfer);
        Ok(Some(Reply::Sealed {
            transfer,
            attachment: sealed.attachment,
            length: sealed.length,
            sha256: sealed.sha256,
            sealed_at: sealed.sealed_at,
        }))
    }

    async fn status(
        &self,
        subject: &Subject,
        transfer: &AttachmentId,
    ) -> Result<Option<Reply>, Fault> {
        let current = self.load(subject, transfer).await?;
        Ok(Some(Reply::Status(status_of(&current))))
    }

    async fn abort(
        &self,
        subject: &Subject,
        transfer: &AttachmentId,
        reason: AbortReason,
        at: Timestamp,
    ) -> Result<Option<Reply>, Fault> {
        let current = self.load(subject, transfer).await?;
        if current.state != AttachmentTransferState::Receiving {
            // 已封存/已取消的传输不可取消（已提交附件按消息保留策略处理）。
            self.forget(subject, transfer);
            return Err(match current.state {
                AttachmentTransferState::Sealed => Fault::Sealed,
                _ => Fault::TransferNotFound,
            });
        }
        let _ = reason;
        let aborted = self
            .transfers
            .abort_transfer(transfer, at)
            .await
            .map_err(|error| match &error {
                PortError::InvalidRequest("transfer is not receiving") => Fault::Sealed,
                PortError::NotFound(_) => Fault::TransferNotFound,
                other => port_fault(other),
            })?;
        self.forget(subject, transfer);
        Ok(Some(Reply::Status(status_of(&aborted))))
    }

    async fn read(
        &self,
        subject: &Subject,
        attachment: AttachmentId,
        session: SessionId,
        offset: u64,
        max_length: u64,
    ) -> Result<Option<Reply>, Fault> {
        let _ = subject;
        if max_length == 0 {
            return Err(Fault::Invalid);
        }
        let chunk = self
            .transfers
            .read_chunk(&AttachmentRead {
                attachment: attachment.clone(),
                session: session.clone(),
                offset,
                max_length,
            })
            .await
            .map_err(|error| match &error {
                PortError::NotFound(_) => Fault::TransferNotFound,
                PortError::InvalidRequest("read offset is past the end") => Fault::Invalid,
                PortError::InvalidRequest("max_length must be positive") => Fault::Invalid,
                other => port_fault(other),
            })?;
        if chunk.data.is_empty() {
            // `offset == totalLength`（EOF 处再读一次）与 0 字节附件都会读到空数据。wire 的
            // `read_chunk.data` 是 `1..=32768` 字节，空数据编不出帧——请求会停在 `Claimed` 里等一个
            // 永不到来的回复。因此把这一情形映射成显式错误（`protocol.schema_invalid`，不可重试）：
            // 客户端必须用新的偏移或接受「该附件没有可读字节」这一事实，而不是重试同一个请求。
            return Err(Fault::Invalid);
        }
        Ok(Some(Reply::ReadChunk {
            attachment,
            session,
            offset: chunk.offset,
            length: chunk.data.len() as u64,
            total_length: chunk.total_length,
            data: chunk.data,
            eof: chunk.eof,
        }))
    }

    async fn manifest_begin(
        &self,
        subject: &Subject,
        submission: SubmissionId,
        session: SessionId,
        declared_subject: &Subject,
        attachment_count: u64,
        at: Timestamp,
    ) -> Result<Option<Reply>, Fault> {
        if declared_subject != subject {
            return Err(Fault::BindingMismatch);
        }
        if attachment_count == 0 {
            return Err(Fault::Invalid);
        }
        let core_subject = subject.core().ok_or(Fault::BindingMismatch)?;
        self.transfers
            .begin_submission(SubmissionBegin {
                submission: submission.clone(),
                session: session.clone(),
                subject: core_subject,
                attachment_count,
                at,
            })
            .await
            .map_err(|error| match &error {
                // 同一 `submissionId` 换了会话/主体/总数：标识复用，不覆盖既有清单。
                PortError::Conflict(_) => Fault::BindingMismatch,
                PortError::NotFound(_) => Fault::TransferNotFound,
                other => port_fault(other),
            })?;
        lock(&self.state).manifests.insert(
            submission.as_str().to_owned(),
            ManifestDraft {
                session,
                subject: subject.clone(),
                declared: attachment_count,
                transfer_ids: Vec::new(),
                final_seen: false,
            },
        );
        Ok(None)
    }

    async fn manifest_page(
        &self,
        subject: &Subject,
        submission: SubmissionId,
        transfer_ids: Vec<AttachmentId>,
        final_page: bool,
    ) -> Result<Option<Reply>, Fault> {
        if transfer_ids.is_empty() || transfer_ids.len() > 64 {
            return Err(Fault::Invalid);
        }
        let (start_position, draft_session) = {
            let state = lock(&self.state);
            let draft = state
                .manifests
                .get(submission.as_str())
                .ok_or(Fault::TransferNotFound)?;
            // 草稿只属于 `manifest_begin` 的那个已认证主体：别的主体不能把页追加进别人的清单。
            if draft.subject != *subject {
                return Err(Fault::BindingMismatch);
            }
            (draft.transfer_ids.len() as u64, draft.session.clone())
        };
        // `SubmissionPage` 要的是**已封存**附件 id：每个 `transferId` 必须先封存，并且必须属于**本清单的
        // 会话与主体**。只检查「已封存」会让另一会话（或已知 id 的其他主体）里的传输被登记进当前清单，
        // 并随后随 prompt 绑定——那正是「跨会话、跨主体复用标识返回 binding_mismatch」要拒绝的情形。
        let mut attachments = Vec::with_capacity(transfer_ids.len());
        for transfer in &transfer_ids {
            let record = self
                .transfers
                .transfer(transfer)
                .await
                .map_err(|_| Fault::Unavailable)?
                .ok_or(Fault::TransferNotFound)?;
            if record.session != draft_session || !subject.matches(&record.subject) {
                return Err(Fault::BindingMismatch);
            }
            let sealed = match record.state {
                AttachmentTransferState::Sealed => record.sealed,
                _ => None,
            };
            let sealed = sealed.ok_or(Fault::Sealed)?;
            attachments.push(sealed.attachment);
        }
        self.transfers
            .append_submission_page(SubmissionPage {
                submission: submission.clone(),
                start_position,
                attachments,
                at: Timestamp::new(self.clock.now().as_str()).map_err(|_| Fault::Unavailable)?,
            })
            .await
            .map_err(|error| match &error {
                PortError::NotFound(_) => Fault::TransferNotFound,
                PortError::Conflict(_) => Fault::ChunkConflict {
                    offset: start_position,
                },
                PortError::InvalidRequest(_) => Fault::Invalid,
                other => port_fault(other),
            })?;
        {
            let mut state = lock(&self.state);
            let draft = state
                .manifests
                .get_mut(submission.as_str())
                .ok_or(Fault::TransferNotFound)?;
            for transfer in transfer_ids {
                if !draft.transfer_ids.contains(&transfer) {
                    draft.transfer_ids.push(transfer);
                }
            }
            if final_page {
                draft.final_seen = true;
            }
        }
        Ok(None)
    }

    async fn manifest_sealed(
        &self,
        subject: &Subject,
        submission: SubmissionId,
        manifest_digest: Digest,
        at: Timestamp,
    ) -> Result<Option<Reply>, Fault> {
        let (session, transfer_ids, declared, final_seen) = {
            let state = lock(&self.state);
            let draft = state
                .manifests
                .get(submission.as_str())
                .ok_or(Fault::TransferNotFound)?;
            if draft.subject != *subject {
                return Err(Fault::BindingMismatch);
            }
            (
                draft.session.clone(),
                draft.transfer_ids.clone(),
                draft.declared,
                draft.final_seen,
            )
        };
        if !final_seen || transfer_ids.len() as u64 != declared {
            return Err(Fault::Invalid);
        }
        // 客户端能看到的清单标识只有 `transferId`，因此 `manifestDigest` 按**清单顺序的 transferId**
        // 计算（长度前缀 + UTF-8，与存储层的清单摘要同为 u32be 前缀的 SHA-256）。这里重新计算而不是
        // 直接采用对端给的值：摘要不符必须显式失败，不能让一份「看起来封存了」的清单进入 prompt。
        let computed = manifest_digest_of(transfer_ids.iter().map(AttachmentId::as_str));
        if computed != manifest_digest {
            return Err(Fault::DigestMismatch);
        }
        let mut attachments = Vec::with_capacity(transfer_ids.len());
        for transfer in &transfer_ids {
            let record = self
                .transfers
                .transfer(transfer)
                .await
                .map_err(|_| Fault::Unavailable)?
                .ok_or(Fault::TransferNotFound)?;
            let sealed = match record.state {
                AttachmentTransferState::Sealed => record.sealed,
                _ => None,
            };
            attachments.push(sealed.ok_or(Fault::Sealed)?.attachment);
        }
        let store_digest = manifest_digest_of(attachments.iter().map(AttachmentId::as_str));
        let submission_row = self
            .transfers
            .finalize_submission(SubmissionSeal {
                submission: submission.clone(),
                session,
                subject: subject.core().ok_or(Fault::BindingMismatch)?,
                attachments,
                digest: store_digest,
                at,
            })
            .await;
        lock(&self.state).manifests.remove(submission.as_str());
        let sealed = submission_row.map_err(|error| match &error {
            PortError::NotFound(_) => Fault::TransferNotFound,
            PortError::Conflict(_) => Fault::Sealed,
            PortError::InvalidRequest(_) => Fault::DigestMismatch,
            other => port_fault(other),
        })?;
        Ok(Some(Reply::ManifestSealed {
            submission,
            attachment_count: sealed.attachments.len() as u64,
            manifest_digest: computed,
            sealed_at: sealed.sealed_at,
        }))
    }

    /// 读一条传输并校验主体绑定；不存在或已失效都把登记从活动集合里移除。
    async fn load(
        &self,
        subject: &Subject,
        transfer: &AttachmentId,
    ) -> Result<AttachmentTransfer, Fault> {
        let record = self
            .transfers
            .transfer(transfer)
            .await
            .map_err(|_| Fault::Unavailable)?
            .ok_or(Fault::TransferNotFound)?;
        if !subject.matches(&record.subject) {
            return Err(Fault::BindingMismatch);
        }
        if record.state != AttachmentTransferState::Receiving
            && record.state != AttachmentTransferState::Sealed
        {
            self.forget(subject, transfer);
        }
        Ok(record)
    }

    fn forget(&self, subject: &Subject, transfer: &AttachmentId) {
        let mut state = lock(&self.state);
        let empty = {
            let Some(registered) = state.receiving.get_mut(&subject.key()) else {
                return;
            };
            registered.remove(transfer.as_str());
            registered.is_empty()
        };
        if empty {
            state.receiving.remove(&subject.key());
        }
    }
}

fn status_of(record: &AttachmentTransfer) -> TransferStatus {
    TransferStatus {
        transfer: record.transfer.clone(),
        state: record.state,
        received_length: record.received_length,
        next_offset: record.received_length,
        sealed_sha256: record.sealed.as_ref().map(|sealed| sealed.sha256.clone()),
    }
}

/// 端口错误 → 中立失败。
fn port_fault(error: &PortError) -> Fault {
    match error {
        PortError::NotFound(_) => Fault::TransferNotFound,
        PortError::Conflict(_) => Fault::ChunkConflict { offset: 0 },
        PortError::InvalidRequest(_) => Fault::Invalid,
        PortError::Unavailable(UnavailableKind::StorageFull) => Fault::BudgetExhausted,
        PortError::Unavailable(_) | PortError::Corrupt(_) | PortError::Backend(_) => {
            Fault::Unavailable
        }
    }
}

/// 清单摘要：`SHA-256( Σ u32be(len(id)) || id )`，base64url 无填充。
///
/// 同一算法在存储层用于**附件 id** 的清单摘要（`owned_attachment_submission.manifest_digest`）；
/// 本函数对 `transferId` 也用它，让客户端按它能看到的标识算出同一个摘要。
pub fn manifest_digest_of<'a>(ids: impl Iterator<Item = &'a str>) -> Digest {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    for id in ids {
        let bytes = id.as_bytes();
        hasher.update((bytes.len() as u32).to_be_bytes());
        hasher.update(bytes);
    }
    let encoded = {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
    };
    Digest::new(&encoded).expect("sha256 digest is canonical base64url")
}

/// 一条 `session.prompt.attachment_ref` 的绑定声明（协议中立）。
///
/// 字段与 wire 的 `attachment_ref` 一一对应，但类型来自 core：两条入站适配器各自从自己的协议 DTO 构造
/// 它，绑定判定只有这里一份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentBinding {
    /// 引用的已封存附件。
    pub attachment: AttachmentId,
    /// 声明的整段 SHA-256。
    pub sha256: Digest,
    /// 声明的字节长度。
    pub byte_length: u64,
}

/// 校验 `session.prompt` 的 `attachment_ref` 是否绑定到**已封存清单**（Sync §11.5 / Node Link §12.7）。
///
/// 三道判定，任一不满足都是 [`Fault::BindingMismatch`]（不区分「不存在」与「不属于你」，不泄露清单或
/// 附件的存在性，也不触发 Agent 执行）：
///
/// 1. `submissionId` 指向的清单必须**已封存**（存储层只把 `sealed` 清单读出来），且会话与主体都等于
///    本次请求的已认证身份；
/// 2. 每个引用的 `attachmentId` 必须在清单的附件集合里；
/// 3. 引用的 `sha256`/`byteLength` 必须与该附件的权威元数据逐字相同。元数据取自**产生该内容的已封存
///    传输行**：内容寻址把同摘要的字节归到一行，而该行由 `seal_transfer` 以当时那个 `transferId` 写入，
///    封存传输行不参与 24 小时清理，因此「附件标识 → 元数据」在本实现里是稳定可读的。客户端在
///    `attachment_ref` 里自报的摘要/长度只是声明，不能作为事实。
pub async fn verify_prompt_bindings(
    transfers: &Arc<dyn AttachmentTransferStore>,
    subject: &Subject,
    session: &SessionId,
    submission: &SubmissionId,
    bindings: &[AttachmentBinding],
) -> Result<(), Fault> {
    let manifest = transfers
        .submission(submission)
        .await
        .map_err(|_| Fault::Unavailable)?
        .ok_or(Fault::BindingMismatch)?;
    if manifest.session != *session || !subject.matches(&manifest.subject) {
        return Err(Fault::BindingMismatch);
    }
    for binding in bindings {
        if !manifest.attachments.contains(&binding.attachment) {
            return Err(Fault::BindingMismatch);
        }
        let record = transfers
            .transfer(&binding.attachment)
            .await
            .map_err(|_| Fault::Unavailable)?
            .ok_or(Fault::BindingMismatch)?;
        let sealed = record
            .sealed
            .filter(|sealed| sealed.attachment == binding.attachment)
            .ok_or(Fault::BindingMismatch)?;
        if sealed.length != binding.byte_length || sealed.sha256 != binding.sha256 {
            return Err(Fault::BindingMismatch);
        }
    }
    Ok(())
}

/// 锁中毒时取回内层数据：预算与草稿的状态在任何 panic 之后仍要可用（失败关闭的方向由调用方决定）。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    //! 4.2 的端口级回归：主体/会话绑定、重复块、封存校验、并发与三层预算、磁盘故障映射。
    //!
    //! 端口替身实现的是 `AttachmentTransferStore` 的**契约**（偏移/摘要/状态），因此这些用例断言的是
    //! 引擎在真实端口错误面上的判定，而不是替身的实现细节。

    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    use acp_core::model::{
        AttachmentSubmission, ConflictKind, EntityRef, SealedAttachment, UnavailableKind,
    };
    use acp_core::ports::{AttachmentChunk, ChunkOutcome, SubmissionProgress};

    const SESSION: &str = "11111111-1111-4111-8111-111111111111";
    const OTHER_SESSION: &str = "22222222-2222-4222-8222-222222222222";
    const AT: &str = "2026-01-01T00:00:00.000Z";

    fn id(number: u32) -> String {
        format!("00000000-0000-4000-8000-{number:012}")
    }

    fn device(number: u32) -> Subject {
        Subject::Device(id(number))
    }

    fn node(number: u32) -> Subject {
        Subject::Node(id(number))
    }

    fn session(text: &str) -> SessionId {
        SessionId::new(text).expect("test session id")
    }

    fn transfer_id(number: u32) -> AttachmentId {
        AttachmentId::new(&id(number)).expect("test transfer id")
    }

    fn submission_id(number: u32) -> SubmissionId {
        SubmissionId::new(&id(number)).expect("test submission id")
    }

    fn begin_request(
        transfer: u32,
        subject: &Subject,
        session_id: &str,
        total: u64,
    ) -> BeginRequest {
        BeginRequest {
            transfer: transfer_id(transfer),
            session: session(session_id),
            subject: subject.clone(),
            total_length: total,
            chunk_size: CHUNK_SIZE as u64,
            media_type: "application/octet-stream".to_owned(),
            file_name: "report.bin".to_owned(),
        }
    }

    fn digest_of(bytes: &[u8]) -> Digest {
        use base64::Engine as _;
        use sha2::Digest as _;
        let encoded =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(bytes));
        Digest::new(&encoded).expect("sha256 is canonical base64url")
    }

    fn chunk(transfer: u32, offset: u64, data: &[u8]) -> Request {
        Request::Chunk {
            transfer: transfer_id(transfer),
            offset,
            data: data.to_vec(),
        }
    }

    fn seal(transfer: u32, data: &[u8]) -> Request {
        Request::Seal {
            transfer: transfer_id(transfer),
            length: data.len() as u64,
            sha256: digest_of(data),
        }
    }

    struct FakeClock;

    impl Clock for FakeClock {
        fn now(&self) -> Timestamp {
            Timestamp::new(AT).expect("test timestamp")
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Failure {
        StorageFull,
        Backend,
    }

    struct FakeTransfer {
        transfer: AttachmentId,
        session: SessionId,
        subject: AttachmentSubject,
        total_length: u64,
        data: Vec<u8>,
        state: AttachmentTransferState,
        sealed: Option<SealedAttachment>,
        created_at: Timestamp,
        updated_at: Timestamp,
    }

    impl FakeTransfer {
        fn project(&self) -> AttachmentTransfer {
            AttachmentTransfer {
                transfer: self.transfer.clone(),
                session: self.session.clone(),
                subject: self.subject.clone(),
                total_length: self.total_length,
                received_length: self.data.len() as u64,
                chunk_size: ATTACHMENT_CHUNK_SIZE,
                state: self.state,
                sealed: self.sealed.clone(),
                created_at: self.created_at.clone(),
                updated_at: self.updated_at.clone(),
            }
        }
    }

    struct FakeSubmission {
        submission: SubmissionId,
        session: SessionId,
        subject: AttachmentSubject,
        declared: u64,
        attachments: Vec<AttachmentId>,
        sealed_at: Option<Timestamp>,
    }

    #[derive(Default)]
    struct FakeStore {
        transfers: Mutex<BTreeMap<String, FakeTransfer>>,
        submissions: Mutex<BTreeMap<String, FakeSubmission>>,
        failure: Mutex<Option<Failure>>,
        /// 为真时 `begin_transfer` 在写行前让出一次执行权。
        ///
        /// 并发 `begin` 用例靠这个让出点暴露 check-then-act：若实现在 `await` 之前没有预留槽位，三个
        /// 并发请求会全部通过预算检查，随后全部落库。真实端口是异步 SQLite 写入，天然存在这个让出点。
        yield_begin: AtomicBool,
    }

    impl FakeStore {
        fn yield_on_begin(&self) {
            self.yield_begin.store(true, Ordering::SeqCst);
        }

        fn fail_with(&self, failure: Failure) {
            *lock(&self.failure) = Some(failure);
        }

        fn clear_failure(&self) {
            *lock(&self.failure) = None;
        }

        fn fail(&self) -> Result<(), PortError> {
            match *lock(&self.failure) {
                Some(Failure::StorageFull) => {
                    Err(PortError::Unavailable(UnavailableKind::StorageFull))
                }
                Some(Failure::Backend) => Err(PortError::Backend("disk is gone".into())),
                None => Ok(()),
            }
        }

        fn missing(transfer: &AttachmentId) -> PortError {
            PortError::NotFound(EntityRef::Attachment(transfer.clone()))
        }
    }

    #[async_trait::async_trait]
    impl AttachmentTransferStore for FakeStore {
        async fn begin_transfer(
            &self,
            write: TransferBegin,
        ) -> Result<AttachmentTransfer, PortError> {
            self.fail()?;
            if self.yield_begin.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            let mut transfers = lock(&self.transfers);
            let key = write.transfer.as_str().to_owned();
            if transfers.contains_key(&key) {
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            let record = FakeTransfer {
                transfer: write.transfer,
                session: write.session,
                subject: write.subject,
                total_length: write.total_length,
                data: Vec::new(),
                state: AttachmentTransferState::Receiving,
                sealed: None,
                created_at: write.at.clone(),
                updated_at: write.at,
            };
            let projection = record.project();
            transfers.insert(key, record);
            Ok(projection)
        }

        async fn append_chunk(&self, write: TransferChunk) -> Result<ChunkOutcome, PortError> {
            self.fail()?;
            let mut transfers = lock(&self.transfers);
            let record = transfers
                .get_mut(write.transfer.as_str())
                .ok_or_else(|| Self::missing(&write.transfer))?;
            if record.state != AttachmentTransferState::Receiving {
                return Err(PortError::InvalidRequest("transfer is not receiving"));
            }
            let received = record.data.len() as u64;
            let end = write.offset.checked_add(write.data.len() as u64).ok_or(
                PortError::InvalidRequest("chunk would exceed the declared total length"),
            )?;
            if write.offset > received {
                return Err(PortError::InvalidRequest(
                    "chunk offset does not match the next expected offset",
                ));
            }
            if end > record.total_length {
                return Err(PortError::InvalidRequest(
                    "chunk would exceed the declared total length",
                ));
            }
            if end <= received {
                let start = write.offset as usize;
                if record.data[start..end as usize] == write.data {
                    return Ok(ChunkOutcome {
                        next_offset: received,
                        duplicate: true,
                    });
                }
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            if write.offset != received {
                return Err(PortError::InvalidRequest(
                    "chunk offset does not match the next expected offset",
                ));
            }
            record.data.extend_from_slice(&write.data);
            record.updated_at = write.at;
            Ok(ChunkOutcome {
                next_offset: record.data.len() as u64,
                duplicate: false,
            })
        }

        async fn seal_transfer(&self, write: TransferSeal) -> Result<SealedAttachment, PortError> {
            self.fail()?;
            let mut transfers = lock(&self.transfers);
            let record = transfers
                .get_mut(write.transfer.as_str())
                .ok_or_else(|| Self::missing(&write.transfer))?;
            if record.state != AttachmentTransferState::Receiving {
                return Err(PortError::InvalidRequest("transfer is not receiving"));
            }
            if write.length != record.data.len() as u64 {
                return Err(PortError::InvalidRequest(
                    "sealed length does not match the received bytes",
                ));
            }
            if write.sha256 != digest_of(&record.data) {
                return Err(PortError::InvalidRequest(
                    "attachment digest does not match the streamed bytes",
                ));
            }
            let sealed = SealedAttachment {
                attachment: record.transfer.clone(),
                length: write.length,
                sha256: write.sha256,
                sealed_at: write.at,
            };
            record.state = AttachmentTransferState::Sealed;
            record.sealed = Some(sealed.clone());
            Ok(sealed)
        }

        async fn transfer(
            &self,
            transfer: &AttachmentId,
        ) -> Result<Option<AttachmentTransfer>, PortError> {
            self.fail()?;
            Ok(lock(&self.transfers)
                .get(transfer.as_str())
                .map(FakeTransfer::project))
        }

        async fn abort_transfer(
            &self,
            transfer: &AttachmentId,
            at: Timestamp,
        ) -> Result<AttachmentTransfer, PortError> {
            self.fail()?;
            let mut transfers = lock(&self.transfers);
            let record = transfers
                .get_mut(transfer.as_str())
                .ok_or_else(|| Self::missing(transfer))?;
            if record.state != AttachmentTransferState::Receiving {
                return Err(PortError::InvalidRequest("transfer is not receiving"));
            }
            record.state = AttachmentTransferState::Aborted;
            record.updated_at = at;
            Ok(record.project())
        }

        async fn begin_submission(
            &self,
            write: SubmissionBegin,
        ) -> Result<SubmissionProgress, PortError> {
            self.fail()?;
            let mut submissions = lock(&self.submissions);
            if submissions.contains_key(write.submission.as_str()) {
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            submissions.insert(
                write.submission.as_str().to_owned(),
                FakeSubmission {
                    submission: write.submission.clone(),
                    session: write.session,
                    subject: write.subject,
                    declared: write.attachment_count,
                    attachments: Vec::new(),
                    sealed_at: None,
                },
            );
            Ok(SubmissionProgress {
                submission: write.submission,
                registered: 0,
                attachment_count: write.attachment_count,
                sealed: false,
            })
        }

        async fn append_submission_page(
            &self,
            write: SubmissionPage,
        ) -> Result<SubmissionProgress, PortError> {
            self.fail()?;
            let mut submissions = lock(&self.submissions);
            let record = submissions
                .get_mut(write.submission.as_str())
                .ok_or_else(|| {
                    PortError::NotFound(EntityRef::Submission(write.submission.clone()))
                })?;
            if write.start_position != record.attachments.len() as u64 {
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            record.attachments.extend(write.attachments);
            Ok(SubmissionProgress {
                submission: write.submission,
                registered: record.attachments.len() as u64,
                attachment_count: record.declared,
                sealed: record.sealed_at.is_some(),
            })
        }

        async fn finalize_submission(
            &self,
            write: SubmissionSeal,
        ) -> Result<AttachmentSubmission, PortError> {
            self.fail()?;
            let mut submissions = lock(&self.submissions);
            let record = submissions
                .get_mut(write.submission.as_str())
                .ok_or_else(|| {
                    PortError::NotFound(EntityRef::Submission(write.submission.clone()))
                })?;
            if record.attachments != write.attachments
                || record.session != write.session
                || record.subject != write.subject
                || record.attachments.len() as u64 != record.declared
            {
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            record.sealed_at = Some(write.at.clone());
            Ok(AttachmentSubmission {
                submission: write.submission,
                session: write.session,
                subject: write.subject,
                attachments: write.attachments,
                digest: write.digest,
                sealed_at: write.at,
            })
        }

        async fn submission(
            &self,
            submission: &SubmissionId,
        ) -> Result<Option<AttachmentSubmission>, PortError> {
            self.fail()?;
            Ok(lock(&self.submissions)
                .get(submission.as_str())
                .and_then(|record| {
                    Some(AttachmentSubmission {
                        submission: record.submission.clone(),
                        session: record.session.clone(),
                        subject: record.subject.clone(),
                        attachments: record.attachments.clone(),
                        digest: manifest_digest_of(
                            record.attachments.iter().map(AttachmentId::as_str),
                        ),
                        sealed_at: record.sealed_at.clone()?,
                    })
                }))
        }

        async fn read_chunk(&self, request: &AttachmentRead) -> Result<AttachmentChunk, PortError> {
            self.fail()?;
            if request.max_length == 0 {
                return Err(PortError::InvalidRequest("max_length must be positive"));
            }
            let transfers = lock(&self.transfers);
            let record = transfers
                .values()
                .find(|record| {
                    record
                        .sealed
                        .as_ref()
                        .is_some_and(|sealed| sealed.attachment == request.attachment)
                })
                .ok_or_else(|| Self::missing(&request.attachment))?;
            // 存储层要求附件绑定到这条会话：跨会话读取一律按「不存在」处理。
            if record.session != request.session {
                return Err(Self::missing(&request.attachment));
            }
            if request.offset > record.data.len() as u64 {
                return Err(PortError::InvalidRequest("read offset is past the end"));
            }
            let start = request.offset as usize;
            let end = (start + request.max_length as usize).min(record.data.len());
            Ok(AttachmentChunk {
                offset: request.offset,
                data: record.data[start..end].to_vec(),
                total_length: record.data.len() as u64,
                eof: end == record.data.len(),
            })
        }

        async fn prune_transfers(&self, _at: Timestamp, _limit: u32) -> Result<u32, PortError> {
            Ok(0)
        }
    }

    fn rig() -> (Arc<Engine>, Arc<FakeStore>) {
        let store = Arc::new(FakeStore::default());
        let engine = Engine::new(store.clone(), Arc::new(FakeClock));
        (engine, store)
    }

    async fn upload(engine: &Engine, subject: &Subject, transfer: u32, data: &[u8]) {
        upload_in(engine, subject, transfer, SESSION, data).await;
    }

    async fn upload_in(
        engine: &Engine,
        subject: &Subject,
        transfer: u32,
        session_id: &str,
        data: &[u8],
    ) {
        let begin = begin_request(transfer, subject, session_id, data.len() as u64);
        assert_eq!(
            engine
                .handle("upload", subject, Request::Begin(begin))
                .await,
            Ok(None)
        );
        assert!(
            engine
                .handle("upload", subject, chunk(transfer, 0, data))
                .await
                .unwrap()
                .is_some()
        );
        assert!(matches!(
            engine.handle("upload", subject, seal(transfer, data)).await,
            Ok(Some(Reply::Sealed { .. }))
        ));
    }

    #[tokio::test]
    async fn upload_seal_and_duplicate_chunk() {
        let (engine, _) = rig();
        let subject = device(1);
        let begin = begin_request(7, &subject, SESSION, 5);
        assert_eq!(
            engine.handle("c1", &subject, Request::Begin(begin)).await,
            Ok(None)
        );
        let first = engine
            .handle("c1", &subject, chunk(7, 0, b"hello"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            first,
            Reply::ChunkAck {
                transfer: transfer_id(7),
                offset: 0,
                length: 5,
                duplicate: false,
                next_offset: 5,
            }
        );
        // 重传同一块：确认是重复而不是再次追加（长度仍是 5）。
        let repeated = engine
            .handle("c1", &subject, chunk(7, 0, b"hello"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            repeated,
            Reply::ChunkAck {
                transfer: transfer_id(7),
                offset: 0,
                length: 5,
                duplicate: true,
                next_offset: 5,
            }
        );
        assert!(matches!(
            engine.handle("c1", &subject, seal(7, b"hello")).await,
            Ok(Some(Reply::Sealed { length: 5, .. }))
        ));
        assert_eq!(engine.active_total(), 0);
        let status = engine
            .handle(
                "c1",
                &subject,
                Request::Status {
                    transfer: transfer_id(7),
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            status,
            Reply::Status(TransferStatus {
                state: AttachmentTransferState::Sealed,
                received_length: 5,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn subject_binding_rejects_foreign_device() {
        let (engine, _) = rig();
        let owner = device(1);
        let intruder = device(2);
        // 帧里自报的主体与已认证主体不同：引擎不采信帧里的声明。
        let mut declared = begin_request(8, &intruder, SESSION, 4);
        declared.subject = intruder.clone();
        assert_eq!(
            engine.handle("c1", &owner, Request::Begin(declared)).await,
            Err(Fault::BindingMismatch)
        );
        upload(&engine, &owner, 8, b"data").await;
        // 别的设备对这条传输的 chunk/status/abort 一律按绑定不符失败。
        assert_eq!(
            engine.handle("c2", &intruder, chunk(8, 4, b"x")).await,
            Err(Fault::BindingMismatch)
        );
        assert_eq!(
            engine
                .handle(
                    "c2",
                    &intruder,
                    Request::Status {
                        transfer: transfer_id(8)
                    }
                )
                .await,
            Err(Fault::BindingMismatch)
        );
        assert_eq!(
            engine
                .handle(
                    "c2",
                    &intruder,
                    Request::Abort {
                        transfer: transfer_id(8),
                        reason: AbortReason::ClientCancelled,
                    }
                )
                .await,
            Err(Fault::BindingMismatch)
        );
    }

    #[tokio::test]
    async fn cross_session_read_is_not_found() {
        let (engine, _) = rig();
        let subject = device(1);
        upload(&engine, &subject, 9, b"payload").await;
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::Read {
                        attachment: transfer_id(9),
                        session: session(OTHER_SESSION),
                        offset: 0,
                        max_length: 16,
                    }
                )
                .await,
            Err(Fault::TransferNotFound)
        );
        let read = engine
            .handle(
                "c1",
                &subject,
                Request::Read {
                    attachment: transfer_id(9),
                    session: session(SESSION),
                    offset: 0,
                    max_length: 16,
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            read,
            Reply::ReadChunk {
                length: 7,
                eof: true,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn duplicate_begin_is_idempotent_and_cross_session_is_rejected() {
        let (engine, _) = rig();
        let subject = device(1);
        let begin = begin_request(10, &subject, SESSION, 3);
        assert_eq!(
            engine
                .handle("c1", &subject, Request::Begin(begin.clone()))
                .await,
            Ok(None)
        );
        assert_eq!(
            engine.handle("c1", &subject, Request::Begin(begin)).await,
            Ok(None)
        );
        assert_eq!(engine.active_total(), 1);
        let reused = begin_request(10, &subject, OTHER_SESSION, 3);
        assert_eq!(
            engine.handle("c1", &subject, Request::Begin(reused)).await,
            Err(Fault::BindingMismatch)
        );
    }

    #[tokio::test]
    async fn active_transfer_budget_per_device_and_node() {
        let (engine, _) = rig();
        let subject = device(1);
        for transfer in 0..2 {
            let begin = begin_request(20 + transfer, &subject, SESSION, 1);
            assert_eq!(
                engine.handle("c1", &subject, Request::Begin(begin)).await,
                Ok(None)
            );
        }
        let overflow = begin_request(30, &subject, SESSION, 1);
        assert_eq!(
            engine
                .handle("c1", &subject, Request::Begin(overflow))
                .await,
            Err(Fault::BudgetExhausted)
        );

        let (engine, _) = rig();
        let owner = node(1);
        for transfer in 0..MAX_ACTIVE_TRANSFERS_PER_NODE {
            let begin = begin_request(100 + transfer as u32, &owner, SESSION, 1);
            assert_eq!(
                engine.handle("c1", &owner, Request::Begin(begin)).await,
                Ok(None)
            );
        }
        let overflow = begin_request(200, &owner, SESSION, 1);
        assert_eq!(
            engine.handle("c1", &owner, Request::Begin(overflow)).await,
            Err(Fault::BudgetExhausted)
        );
    }

    #[tokio::test]
    async fn concurrent_begins_respect_the_device_budget() {
        let (engine, _) = rig();
        let subject = device(1);
        let attempts: Vec<_> = (0..3)
            .map(|index| {
                let engine = Arc::clone(&engine);
                let subject = subject.clone();
                async move {
                    let connection = format!("c{index}");
                    engine
                        .handle(
                            &connection,
                            &subject,
                            Request::Begin(begin_request(40 + index as u32, &subject, SESSION, 1)),
                        )
                        .await
                }
            })
            .collect();
        let mut accepted = 0;
        let mut rejected = 0;
        for result in futures_util::future::join_all(attempts).await {
            match result {
                Ok(None) => accepted += 1,
                Err(Fault::BudgetExhausted) => rejected += 1,
                other => panic!("unexpected result {other:?}"),
            }
        }
        // 并发下也只能有 2 条进入 receiving：第三条被预算拒绝。
        assert_eq!((accepted, rejected), (2, 1));
        assert_eq!(engine.active_total(), 2);
    }

    #[tokio::test]
    async fn frame_budget_is_per_connection_and_retryable() {
        let (engine, _) = rig();
        let subject = device(1);
        for _ in 0..FRAMES_PER_MINUTE {
            // 传输不存在也是合法请求：预算在进入引擎前按帧计数。
            let _ = engine
                .handle(
                    "c1",
                    &subject,
                    Request::Status {
                        transfer: transfer_id(999),
                    },
                )
                .await;
        }
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::Status {
                        transfer: transfer_id(999),
                    }
                )
                .await,
            Err(Fault::BudgetExhausted)
        );
        assert!(Fault::BudgetExhausted.retryable());
        // 另一个附件的连接（独立连接）不受影响。
        assert_eq!(
            engine
                .handle(
                    "c2",
                    &subject,
                    Request::Status {
                        transfer: transfer_id(999),
                    }
                )
                .await,
            Err(Fault::TransferNotFound)
        );
    }

    #[tokio::test]
    async fn storage_failures_map_to_retryable_faults() {
        let (engine, store) = rig();
        let subject = device(1);
        store.fail_with(Failure::StorageFull);
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::Begin(begin_request(50, &subject, SESSION, 1))
                )
                .await,
            Err(Fault::BudgetExhausted)
        );
        store.fail_with(Failure::Backend);
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::Begin(begin_request(51, &subject, SESSION, 1))
                )
                .await,
            Err(Fault::Unavailable)
        );
        assert!(Fault::Unavailable.retryable());
        store.clear_failure();
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::Status {
                        transfer: transfer_id(51),
                    }
                )
                .await,
            Err(Fault::TransferNotFound)
        );
    }

    #[tokio::test]
    async fn seal_with_wrong_digest_is_rejected() {
        let (engine, _) = rig();
        let subject = device(1);
        let begin = begin_request(60, &subject, SESSION, 3);
        engine
            .handle("c1", &subject, Request::Begin(begin))
            .await
            .unwrap();
        engine
            .handle("c1", &subject, chunk(60, 0, b"abc"))
            .await
            .unwrap();
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::Seal {
                        transfer: transfer_id(60),
                        length: 3,
                        sha256: digest_of(b"xyz"),
                    }
                )
                .await,
            Err(Fault::DigestMismatch)
        );
    }

    #[tokio::test]
    async fn manifest_binds_subject_session_and_digest() {
        let (engine, _) = rig();
        let subject = device(1);
        upload(&engine, &subject, 70, b"one").await;
        upload(&engine, &subject, 71, b"two").await;
        let submission = submission_id(700);
        let begin = Request::ManifestBegin {
            submission: submission.clone(),
            session: session(SESSION),
            subject: subject.clone(),
            attachment_count: 2,
        };
        assert_eq!(engine.handle("c1", &subject, begin).await, Ok(None));

        // 另一主体不能接管这份清单。
        assert_eq!(
            engine
                .handle(
                    "c2",
                    &device(2),
                    Request::ManifestBegin {
                        submission: submission.clone(),
                        session: session(SESSION),
                        subject: device(2),
                        attachment_count: 2,
                    }
                )
                .await,
            Err(Fault::BindingMismatch)
        );
        // 同一节点/主体但换了会话：标识复用，显式拒绝。
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::ManifestBegin {
                        submission: submission.clone(),
                        session: session(OTHER_SESSION),
                        subject: subject.clone(),
                        attachment_count: 2,
                    }
                )
                .await,
            Err(Fault::BindingMismatch)
        );
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::ManifestPage {
                        submission: submission.clone(),
                        transfer_ids: vec![transfer_id(70), transfer_id(71)],
                        final_page: true,
                    }
                )
                .await,
            Ok(None)
        );
        let wrong = Digest::new(digest_of(b"wrong").as_str()).unwrap();
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::ManifestSealed {
                        submission: submission.clone(),
                        manifest_digest: wrong,
                    }
                )
                .await,
            Err(Fault::DigestMismatch)
        );
        let correct =
            manifest_digest_of([transfer_id(70).as_str(), transfer_id(71).as_str()].into_iter());
        let sealed = engine
            .handle(
                "c1",
                &subject,
                Request::ManifestSealed {
                    submission: submission.clone(),
                    manifest_digest: correct.clone(),
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            sealed,
            Reply::ManifestSealed {
                attachment_count: 2,
                ..
            }
        ));
        // 清单封存后草稿被清掉：再次封存是同一种「不存在」失败。
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::ManifestSealed {
                        submission,
                        manifest_digest: correct.clone(),
                    }
                )
                .await,
            Err(Fault::TransferNotFound)
        );
    }

    /// 只封存不裁页：清单页接受**本清单会话/主体**里的传输，别处的已封存传输明确拒绝。
    ///
    /// 「已封存」不是归属证明：同一 Node/设备把另一会话（或已知 id 的其他主体）里的 `transferId` 放进
    /// 当前 submission 时，若只看状态，这份跨会话/跨主体的引用就会被接受并随后绑定到 prompt。
    #[tokio::test]
    async fn manifest_page_rejects_transfers_outside_the_draft_binding() {
        let (engine, _) = rig();
        let subject = device(1);
        // 另一会话里的传输 + 另一主体在同一会话里的传输。
        upload_in(&engine, &subject, 80, OTHER_SESSION, b"other").await;
        upload(&engine, &device(2), 81, b"foreign").await;
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::ManifestBegin {
                        submission: submission_id(800),
                        session: session(SESSION),
                        subject: subject.clone(),
                        attachment_count: 1,
                    }
                )
                .await,
            Ok(None)
        );
        for transfer in [80, 81] {
            assert_eq!(
                engine
                    .handle(
                        "c1",
                        &subject,
                        Request::ManifestPage {
                            submission: submission_id(800),
                            transfer_ids: vec![transfer_id(transfer)],
                            final_page: true,
                        }
                    )
                    .await,
                Err(Fault::BindingMismatch),
                "transfer {transfer} is not part of this draft binding"
            );
        }
        // 清单草稿只属于 `manifest_begin` 的主体：别的设备不能往别人的清单里追加页。
        assert_eq!(
            engine
                .handle(
                    "c2",
                    &device(2),
                    Request::ManifestPage {
                        submission: submission_id(800),
                        transfer_ids: vec![transfer_id(81)],
                        final_page: true,
                    }
                )
                .await,
            Err(Fault::BindingMismatch)
        );
    }

    /// 连接放弃（断线/关闭）必须释放它预留的活动传输槽位：两次放弃之后仍能继续上传。
    #[tokio::test]
    async fn abandoned_connections_release_their_active_transfer_slots() {
        let (engine, _) = rig();
        let subject = device(1);
        for transfer in 0..2 {
            assert_eq!(
                engine
                    .handle(
                        "abandoned",
                        &subject,
                        Request::Begin(begin_request(90 + transfer, &subject, SESSION, 1))
                    )
                    .await,
                Ok(None)
            );
        }
        assert_eq!(
            engine
                .handle(
                    "abandoned",
                    &subject,
                    Request::Begin(begin_request(99, &subject, SESSION, 1))
                )
                .await,
            Err(Fault::BudgetExhausted)
        );
        engine.release_connection("abandoned");
        assert_eq!(engine.active_total(), 0);
        // 第一次放弃后可以再开两条；再放弃一次也一样（预算不会被过去的连接永久占住）。
        for attempt in 0..2 {
            let connection = format!("retry{attempt}");
            for transfer in 0..2 {
                assert_eq!(
                    engine
                        .handle(
                            &connection,
                            &subject,
                            Request::Begin(begin_request(
                                100 + attempt * 10 + transfer,
                                &subject,
                                SESSION,
                                1
                            ))
                        )
                        .await,
                    Ok(None)
                );
            }
            assert_eq!(engine.active_total(), 2);
            engine.release_connection(&connection);
            assert_eq!(engine.active_total(), 0);
        }
        // 只释放自己的登记：另一条连接上的传输不受影响。
        assert_eq!(
            engine
                .handle(
                    "first",
                    &subject,
                    Request::Begin(begin_request(130, &subject, SESSION, 1))
                )
                .await,
            Ok(None)
        );
        assert_eq!(
            engine
                .handle(
                    "second",
                    &subject,
                    Request::Begin(begin_request(131, &subject, SESSION, 1))
                )
                .await,
            Ok(None)
        );
        engine.release_connection("first");
        assert_eq!(engine.active_total(), 1);
        assert_eq!(
            engine
                .handle(
                    "third",
                    &subject,
                    Request::Begin(begin_request(132, &subject, SESSION, 1))
                )
                .await,
            Ok(None)
        );
        // 第二条连接仍占着另一条传输：预算只剩 0。
        assert_eq!(
            engine
                .handle(
                    "third",
                    &subject,
                    Request::Begin(begin_request(133, &subject, SESSION, 1))
                )
                .await,
            Err(Fault::BudgetExhausted)
        );
    }

    /// `offset == totalLength`（EOF 再读）与 0 字节附件都读到空数据：必须回显式错误，不得「已认领但
    /// 没有回帧」（空 `data` 编不出 `read_chunk`）。
    #[tokio::test]
    async fn reads_past_the_end_and_empty_attachments_fail_explicitly() {
        let (engine, _) = rig();
        let subject = device(1);
        upload(&engine, &subject, 140, b"abc").await;
        for offset in [3, 4] {
            assert_eq!(
                engine
                    .handle(
                        "c1",
                        &subject,
                        Request::Read {
                            attachment: transfer_id(140),
                            session: session(SESSION),
                            offset,
                            max_length: 8,
                        }
                    )
                    .await,
                Err(Fault::Invalid),
                "offset {offset} has no bytes to return"
            );
        }
        // 0 字节附件：`begin`/`seal` 允许，但读取没有可编码的块。
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::Begin(begin_request(141, &subject, SESSION, 0))
                )
                .await,
            Ok(None)
        );
        assert!(matches!(
            engine.handle("c1", &subject, seal(141, b"")).await,
            Ok(Some(Reply::Sealed { length: 0, .. }))
        ));
        assert_eq!(
            engine
                .handle(
                    "c1",
                    &subject,
                    Request::Read {
                        attachment: transfer_id(141),
                        session: session(SESSION),
                        offset: 0,
                        max_length: 8,
                    }
                )
                .await,
            Err(Fault::Invalid)
        );
    }

    /// 并发 `begin`：预算检查与槽位预留在同一临界区。端口在写行前让出执行权（真实 SQLite 写入天然如此），
    /// 若实现是 check-then-act，三个请求会全部通过检查并全部落库。
    #[tokio::test]
    async fn concurrent_begins_reserve_the_slot_before_awaiting_the_store() {
        let (engine, store) = rig();
        store.yield_on_begin();
        let subject = device(1);
        let attempts: Vec<_> = (0..3)
            .map(|index| {
                let engine = Arc::clone(&engine);
                let subject = subject.clone();
                async move {
                    let connection = format!("c{index}");
                    engine
                        .handle(
                            &connection,
                            &subject,
                            Request::Begin(begin_request(150 + index as u32, &subject, SESSION, 1)),
                        )
                        .await
                }
            })
            .collect();
        let mut accepted = 0;
        let mut rejected = 0;
        for result in futures_util::future::join_all(attempts).await {
            match result {
                Ok(None) => accepted += 1,
                Err(Fault::BudgetExhausted) => rejected += 1,
                other => panic!("unexpected result {other:?}"),
            }
        }
        assert_eq!((accepted, rejected), (2, 1));
        assert_eq!(engine.active_total(), 2);
    }

    fn binding(attachment: u32, data: &[u8]) -> AttachmentBinding {
        AttachmentBinding {
            attachment: transfer_id(attachment),
            sha256: digest_of(data),
            byte_length: data.len() as u64,
        }
    }

    /// 上传 + 封存 + 单附件清单封存；返回清单里的附件标识。
    async fn sealed_manifest(
        engine: &Engine,
        subject: &Subject,
        submission: u32,
        transfer: u32,
        data: &[u8],
    ) -> AttachmentId {
        upload(engine, subject, transfer, data).await;
        assert_eq!(
            engine
                .handle(
                    "manifest",
                    subject,
                    Request::ManifestBegin {
                        submission: submission_id(submission),
                        session: session(SESSION),
                        subject: subject.clone(),
                        attachment_count: 1,
                    }
                )
                .await,
            Ok(None)
        );
        assert_eq!(
            engine
                .handle(
                    "manifest",
                    subject,
                    Request::ManifestPage {
                        submission: submission_id(submission),
                        transfer_ids: vec![transfer_id(transfer)],
                        final_page: true,
                    }
                )
                .await,
            Ok(None)
        );
        let digest = manifest_digest_of([transfer_id(transfer).as_str()].into_iter());
        assert!(matches!(
            engine
                .handle(
                    "manifest",
                    subject,
                    Request::ManifestSealed {
                        submission: submission_id(submission),
                        manifest_digest: digest,
                    }
                )
                .await,
            Ok(Some(Reply::ManifestSealed { .. }))
        ));
        transfer_id(transfer)
    }

    /// `session.prompt` 的 `attachment_ref` 绑定校验：清单必须已封存且属于本会话/主体，逐块元数据必须
    /// 与已封存传输一致；任何一项不符都返回 `binding_mismatch`（不泄露存在性）。
    #[tokio::test]
    async fn prompt_attachment_bindings_must_match_the_sealed_manifest() {
        let (engine, store) = rig();
        let subject = device(1);
        let attachment = sealed_manifest(&engine, &subject, 300, 301, b"hello").await;
        let store: Arc<dyn AttachmentTransferStore> = store;
        let submission = submission_id(300);
        let session_id = session(SESSION);
        let bound = binding(301, b"hello");
        assert_eq!(attachment, bound.attachment);
        assert_eq!(
            verify_prompt_bindings(
                &store,
                &subject,
                &session_id,
                &submission,
                std::slice::from_ref(&bound),
            )
            .await,
            Ok(())
        );
        // 声明的长度/摘要不符（客户端自报值不是事实）。
        let long = AttachmentBinding {
            byte_length: 6,
            ..bound.clone()
        };
        assert_eq!(
            verify_prompt_bindings(&store, &subject, &session_id, &submission, &[long]).await,
            Err(Fault::BindingMismatch)
        );
        let other_digest = AttachmentBinding {
            sha256: digest_of(b"other"),
            ..bound
        };
        assert_eq!(
            verify_prompt_bindings(&store, &subject, &session_id, &submission, &[other_digest])
                .await,
            Err(Fault::BindingMismatch)
        );
        // 清单之外的附件（已封存，但没登记进这份清单）。
        upload(&engine, &subject, 302, b"outside").await;
        assert_eq!(
            verify_prompt_bindings(
                &store,
                &subject,
                &session_id,
                &submission,
                &[binding(302, b"outside")]
            )
            .await,
            Err(Fault::BindingMismatch)
        );
        // 会话或主体不符。
        assert_eq!(
            verify_prompt_bindings(
                &store,
                &subject,
                &session(OTHER_SESSION),
                &submission,
                &[binding(301, b"hello")]
            )
            .await,
            Err(Fault::BindingMismatch)
        );
        assert_eq!(
            verify_prompt_bindings(
                &store,
                &device(2),
                &session_id,
                &submission,
                &[binding(301, b"hello")]
            )
            .await,
            Err(Fault::BindingMismatch)
        );
        // 不存在/未封存的清单。
        assert_eq!(
            verify_prompt_bindings(
                &store,
                &subject,
                &session_id,
                &submission_id(999),
                &[binding(301, b"hello")]
            )
            .await,
            Err(Fault::BindingMismatch)
        );
    }
}
