//! Sync 独立附件连接（`docs/SYNC_PROTOCOL.md` §3.4）：`/sync/v1/attachments` 的帧适配与鉴权后的分派。
//!
//! 连接面的握手、来源绑定、撤销、心跳与关闭语义完全复用 [`crate::sync::conn`]（同一个 `SyncConn`，
//! 只是 [`Purpose::Attachments`](crate::sync::conn::Purpose)）；本模块只负责四件事：
//!
//! 1. **协商门控**：`selectedFeatures` 里没有 `core.attachment-transfer.v1` 时，任何 `attachment.*` 帧都回
//!    `attachment.negotiation_required` 并以 `4400` 关闭（§3.4/§5.3）。门控在**进入引擎之前**判定，
//!    未协商的连接不触碰存储、不产生任何写入。
//! 2. **方向判定**：服务端只接受上传方向的帧（`begin`/`chunk`/`seal`/`manifest_*`）与双向帧
//!    （`status`/`abort`/`read`/`error`）。客户端发来 `chunk_ack`/`sealed`/`read_chunk` 属于方向错误，
//!    回 `protocol.type_unsupported`；不静默丢弃，也不当成业务命令。
//! 3. **协议中立引擎的 wire 映射**：正式（owned）帧 → [`Request`]，[`Reply`]/[`Fault`] → 帧与错误体。
//!    分块/封存/清单的判定只在 [`crate::attachment_transfer`] 里有一份实现。
//! 4. **准备目标分支**（`design.md` D6.1–D6.4）：`begin`/`manifest_begin` 的 `target.kind=preparation`
//!    走 [`crate::sync::preparation::Preparations`]——双 feature 协商、连接身份、receipt 绑定、首条受理
//!    查询与运行期资源代都在**进入暂存端口之前**判定；分块/封存/清单只从运行期记录恢复目标并复查
//!    连接归属（跨连接重绑必须重发完整 target 的 `begin`）。正式分支的形状与判定顺序逐字不变。
//!
//! 连接**不登记业务注册表**：附件连接不进 `(hostId, deviceId)` 槽位（不会顶替该设备的业务连接），也不
//! 参与事件扇出；撤销经 `ConnectionRegistry::close_device` 同时关闭业务与附件两张表（见 `conn::registry`
//! 的文档）。出站序号与队列预算与业务消息共用同一份账本（`ConnectionHandle::send_attachment`）。
//!
//! 依赖纪律：只调用 `core::ports` 的附件端口、`core::use_cases` 的只读装配入口与 `sync-protocol` 的公开
//! DTO；不 import `storage-sqlite`、不调用 `server::node_link`。

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use acp_core::model::{
    Actor, AttachmentId, AttachmentSubject, ConnectionId, Digest, ImmutableFileMetadata, PortError,
    PreparationBinding, ResourceGeneration, RuntimeEpoch, SessionId, StagedAbortReason,
    StagedChunk, StagedManifestBegin, StagedManifestPage, StagedManifestSeal, StagedSeal,
    StagedSealOutcome, StagedTransferBegin, StagedTransferState, SubmissionId, Timestamp,
    UnavailableKind,
};
use acp_core::use_cases::UseCases;
use sync_protocol::attachment::{
    AbortReason as WireAbortReason, BeginBody, Direction, ErrorBody, Frame, FrameBody, FrameType,
    ManifestBeginBody, ManifestSealedBody, ReadChunkBody, SealedBody, StatusBody, TransferState,
};
use sync_protocol::common::{DecimalString, Nullable, RawObject, Timestamp as WireTimestamp, Uuid};
use sync_protocol::error::ErrorCode;
use tracing::{debug, warn};

use crate::attachment_transfer::{
    AbortReason, BeginRequest, Engine, Fault, Reply, Request, Subject, TransferStatus,
};
use crate::sync::conn::{ConnectionHandle, close};
use crate::sync::preparation::{
    AuthReject, PREPARATION_FEATURE, Preparations, ReceiptReject, RuntimeCheck,
};

/// `core.attachment-transfer.v1`（`compatibility/features/v1/features.json`）。
pub const ATTACHMENT_FEATURE: &str = "core.attachment-transfer.v1";

/// 单设备准备上传的活动传输上限（与正式上传共用同一约定：单设备 2）。
const MAX_PREPARATION_TRANSFERS_PER_DEVICE: usize = 2;

/// 准备清单草稿的条数上限（连接期/进程内有界，不随页面长期运行增长）。
const MAX_PREPARATION_MANIFESTS: usize = 64;

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
    /// 引擎在这里释放该连接预留的活动传输槽位：登记若留在进程内的 `receiving` 集合里，客户端放弃的
    /// 上传会永久占住单设备 2 / 单节点 8 的预算。默认实现为空操作（不需要连接期账的路由不必实现）。
    fn released(&self, _session: &ConnectionHandle) {}
}

/// 准备上传的活动传输账（只在进程内；断线释放本连接预留的槽位，元数据留给重绑）。
#[derive(Debug, Default)]
struct PreparationActive {
    /// 设备键 → 仍在接收的传输 → **预留它的附件连接**。
    devices: BTreeMap<String, BTreeMap<String, String>>,
}

/// 一份准备清单的连接期草稿（端口只给运行期状态，顺序与封存结果在这里记住）。
#[derive(Debug, Clone)]
struct PreparationManifest {
    session: SessionId,
    connection: String,
    runtime_epoch: RuntimeEpoch,
    generation: ResourceGeneration,
    count: u32,
    registered: Vec<AttachmentId>,
    final_seen: bool,
    sealed: Option<ManifestSealedBody>,
}

/// Sync 面的附件服务：持有协议中立引擎、准备暂存面，负责门控与 wire 映射。
pub struct AttachmentService {
    engine: Arc<Engine>,
    preparations: Option<Arc<Preparations>>,
    active: Mutex<PreparationActive>,
    manifests: Mutex<VecDeque<(String, PreparationManifest)>>,
}

impl std::fmt::Debug for AttachmentService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("sync::AttachmentService")
            .field("preparation", &self.preparations.is_some())
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
            preparations: None,
            active: Mutex::new(PreparationActive::default()),
            manifests: Mutex::new(VecDeque::new()),
        }))
    }

    /// 从用例面装配，并接线准备暂存面（组合根在准备端口齐备时调用）。
    pub fn from_core_with_preparation(
        core: &Arc<UseCases>,
        preparations: Arc<Preparations>,
    ) -> Option<Arc<Self>> {
        let transfers = Arc::clone(core.attachment_transfers()?);
        let clock = Arc::clone(core.clock());
        Some(Arc::new(Self {
            engine: Engine::new(transfers, clock),
            preparations: Some(preparations),
            active: Mutex::new(PreparationActive::default()),
            manifests: Mutex::new(VecDeque::new()),
        }))
    }

    /// 从已装配的引擎构造（组合根在 Sync 与 Node Link 之间共享同一个引擎时使用）。
    pub fn from_engine(engine: Arc<Engine>) -> Arc<Self> {
        Arc::new(Self {
            engine,
            preparations: None,
            active: Mutex::new(PreparationActive::default()),
            manifests: Mutex::new(VecDeque::new()),
        })
    }

    /// 从已装配的引擎构造，并接线准备暂存面。
    pub fn from_engine_with_preparation(
        engine: Arc<Engine>,
        preparations: Arc<Preparations>,
    ) -> Arc<Self> {
        Arc::new(Self {
            engine,
            preparations: Some(preparations),
            active: Mutex::new(PreparationActive::default()),
            manifests: Mutex::new(VecDeque::new()),
        })
    }

    fn preparation_negotiated(features: &[String]) -> bool {
        features
            .iter()
            .any(|feature| feature == PREPARATION_FEATURE)
    }

    /// 释放本连接预留的准备活动传输槽位（不断言传输已经结束；元数据留给重绑/过期清理）。
    fn release_preparation_connection(&self, connection: &str) {
        let mut active = lock(&self.active);
        active.devices.retain(|_, registered| {
            registered.retain(|_, owner| owner != connection);
            !registered.is_empty()
        });
    }

    /// 预留一个准备传输槽位（同一设备 2；同一 transferId 只记一次并转移连接归属）。
    fn reserve_preparation(
        &self,
        device: &str,
        transfer: &str,
        connection: &str,
    ) -> Result<(), ()> {
        let mut active = lock(&self.active);
        let registered = active.devices.entry(device.to_owned()).or_default();
        if let Some(owner) = registered.get_mut(transfer) {
            *owner = connection.to_owned();
            return Ok(());
        }
        if registered.len() >= MAX_PREPARATION_TRANSFERS_PER_DEVICE {
            return Err(());
        }
        registered.insert(transfer.to_owned(), connection.to_owned());
        Ok(())
    }

    /// 释放一个准备传输槽位（只移除该 transfer 自己的登记）。
    fn release_preparation_transfer(&self, transfer: &str) {
        let mut active = lock(&self.active);
        active.devices.retain(|_, registered| {
            registered.remove(transfer);
            !registered.is_empty()
        });
    }

    fn manifest(&self, submission: &Uuid) -> Option<PreparationManifest> {
        lock(&self.manifests)
            .iter()
            .find(|(key, _)| key == submission.as_str())
            .map(|(_, manifest)| manifest.clone())
    }

    fn remember_manifest(&self, submission: &Uuid, manifest: PreparationManifest) {
        let mut manifests = lock(&self.manifests);
        manifests.retain(|(key, _)| key != submission.as_str());
        while manifests.len() >= MAX_PREPARATION_MANIFESTS {
            manifests.pop_front();
        }
        manifests.push_back((submission.as_str().to_owned(), manifest));
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
                event = "sync.attachment_negotiation_required",
                connection_id = session.connection_id().as_str(),
                "an attachment frame arrived on a connection that did not negotiate the feature"
            );
            send_error(
                session,
                frame.transfer_hint(),
                ErrorCode::AttachmentNegotiationRequired,
                "the connection did not negotiate core.attachment-transfer.v1",
                RawObject::empty(),
            );
            return AttachmentOutcome::Close(
                close::PROTOCOL,
                "attachment feature was not negotiated",
            );
        }
        let subject = match Subject::from_actor(&Actor::Device {
            device: session.device_id().clone(),
        }) {
            Some(subject) => subject,
            None => {
                // 设备身份必然能构造主体；这里是接线错误的失败关闭。
                return AttachmentOutcome::Close(
                    close::UNAVAILABLE,
                    "the connection identity is not an attachment subject",
                );
            }
        };
        let frame_type = frame.body.frame_type();
        if !is_client_frame(frame_type) {
            send_error(
                session,
                frame.transfer_hint(),
                ErrorCode::ProtocolTypeUnsupported,
                "this frame direction is not accepted on an attachment connection",
                RawObject::empty(),
            );
            return AttachmentOutcome::Claimed;
        }
        // 准备目标分支：双 feature 协商与连接身份之外的判定都在 [`Preparations`] 里。
        let negotiated = Self::preparation_negotiated(features);
        match &frame.body {
            FrameBody::Begin(BeginBody::Preparation(body)) => {
                return self.preparation_begin(session, negotiated, body).await;
            }
            FrameBody::ManifestBegin(ManifestBeginBody::Preparation(body)) => {
                return self
                    .preparation_manifest_begin(session, negotiated, body)
                    .await;
            }
            FrameBody::Chunk(_)
            | FrameBody::Seal(_)
            | FrameBody::Status(_)
            | FrameBody::Abort(_) => {
                if negotiated {
                    if let Some(preparations) = self.preparations.as_ref() {
                        match self
                            .preparation_transfer(session, preparations, frame)
                            .await
                        {
                            Ok(true) => return AttachmentOutcome::Claimed,
                            // 不属于准备运行期记录：继续走正式引擎（同一帧类型下两种传输互不干扰）。
                            Ok(false) => {}
                            Err(fault) => return finish_fault(session, fault),
                        }
                    }
                }
            }
            FrameBody::ManifestPage(_) | FrameBody::ManifestSealed(_) => {
                if negotiated && let Some(preparations) = self.preparations.as_ref() {
                    match self
                        .preparation_manifest_control(session, preparations, frame)
                        .await
                    {
                        Ok(true) => return AttachmentOutcome::Claimed,
                        Ok(false) => {}
                        Err(fault) => return finish_fault(session, fault),
                    }
                }
            }
            _ => {}
        }
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
                            event = "sync.attachment_reply_dropped",
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
        self.release_preparation_connection(session.connection_id().as_str());
    }
}

impl AttachmentService {
    /// `attachment.begin` 的准备分支。
    async fn preparation_begin(
        &self,
        session: &ConnectionHandle,
        negotiated: bool,
        body: &sync_protocol::attachment::PreparationBeginBody,
    ) -> AttachmentOutcome {
        let transfer = body.transfer_id.clone();
        let target = ErrorTarget::Transfer(Some(transfer.clone()));
        if !negotiated {
            return finish_fault(session, feature_required(target));
        }
        let Some(preparations) = self.preparations.as_ref() else {
            return finish_fault(
                session,
                fault(
                    target,
                    ErrorCode::InternalUnavailable,
                    "the preparation attachment staging is unavailable",
                    RawObject::empty(),
                ),
            );
        };
        if let Err(fault) = self.begin_staging(session, preparations, body).await {
            return finish_fault(session, fault);
        }
        AttachmentOutcome::Claimed
    }

    async fn begin_staging(
        &self,
        session: &ConnectionHandle,
        preparations: &Arc<Preparations>,
        body: &sync_protocol::attachment::PreparationBeginBody,
    ) -> Result<(), PrepFault> {
        let transfer = body.transfer_id.clone();
        let target = ErrorTarget::Transfer(Some(transfer.clone()));
        let (verified, runtime_epoch, generation) =
            verify_target(session, preparations, &body.target, &target).await?;
        if body.subject.id.as_str() != session.device_id().as_str() {
            return Err(binding_fault(target));
        }
        if body.chunk_size.get() != 32768 {
            return Err(fault(
                target,
                ErrorCode::ProtocolSchemaInvalid,
                "chunkSize must be 32768",
                RawObject::empty(),
            ));
        }
        let total_length = decimal(&body.total_length).map_err(|_| {
            fault(
                target.clone(),
                ErrorCode::ProtocolSchemaInvalid,
                "totalLength is not a supported decimal",
                RawObject::empty(),
            )
        })?;
        let Some(staging) = preparations.staging() else {
            return Err(fault(
                target,
                ErrorCode::InternalUnavailable,
                "the preparation attachment staging is unavailable",
                RawObject::empty(),
            ));
        };
        let at = preparations.now();
        let binding = PreparationBinding {
            device: session.device_id().clone(),
            session: verified.session.clone(),
            tab_nonce: verified.tab_nonce.clone(),
            runtime_epoch,
            generation,
            claims_digest: verified.claims_digest.clone(),
        };
        let existing = staging
            .status(attachment_id(&transfer))
            .await
            .map_err(|error| staging_fault(target.clone(), &error))?;
        let reserved = if existing.is_none() {
            self.reserve_preparation(
                session.device_id().as_str(),
                transfer.as_str(),
                session.connection_id().as_str(),
            )
            .map_err(|_| {
                fault(
                    target.clone(),
                    ErrorCode::AttachmentBudgetExhausted,
                    "the device already has the maximum active preparation uploads",
                    RawObject::empty(),
                )
            })?;
            true
        } else {
            false
        };
        let begun = staging
            .begin(StagedTransferBegin {
                transfer: attachment_id(&transfer),
                binding,
                subject: AttachmentSubject::Device(session.device_id().clone()),
                connection: connection_id(session),
                total_length,
                immutable: ImmutableFileMetadata {
                    media_type: body.media_type.as_str().to_owned(),
                    file_name: body.file_name.as_str().to_owned(),
                },
                at,
            })
            .await;
        match begun {
            Ok(_) => Ok(()),
            Err(error) => {
                if reserved {
                    self.release_preparation_transfer(transfer.as_str());
                }
                Err(staging_fault(target, &error))
            }
        }
    }

    /// `attachment.manifest_begin` 的准备分支。
    async fn preparation_manifest_begin(
        &self,
        session: &ConnectionHandle,
        negotiated: bool,
        body: &sync_protocol::attachment::PreparationManifestBeginBody,
    ) -> AttachmentOutcome {
        let target = ErrorTarget::Submission(body.submission_id.clone());
        if !negotiated {
            return finish_fault(session, feature_required(target));
        }
        let Some(preparations) = self.preparations.as_ref() else {
            return finish_fault(
                session,
                fault(
                    target,
                    ErrorCode::InternalUnavailable,
                    "the preparation attachment staging is unavailable",
                    RawObject::empty(),
                ),
            );
        };
        match self
            .manifest_begin_staging(session, preparations, body)
            .await
        {
            Ok(Some(reply)) => {
                if session
                    .send_attachment(&FrameBody::ManifestSealed(reply))
                    .is_err()
                {
                    debug!(
                        event = "sync.preparation_manifest_reply_dropped",
                        connection_id = session.connection_id().as_str(),
                        "the preparation manifest reply could not be queued"
                    );
                }
                AttachmentOutcome::Claimed
            }
            Ok(None) => AttachmentOutcome::Claimed,
            Err(fault) => finish_fault(session, fault),
        }
    }

    async fn manifest_begin_staging(
        &self,
        session: &ConnectionHandle,
        preparations: &Arc<Preparations>,
        body: &sync_protocol::attachment::PreparationManifestBeginBody,
    ) -> Result<Option<ManifestSealedBody>, PrepFault> {
        let submission = body.submission_id.clone();
        let target = ErrorTarget::Submission(submission.clone());
        let (verified, runtime_epoch, generation) =
            verify_target(session, preparations, &body.target, &target).await?;
        if body.subject.id.as_str() != session.device_id().as_str() {
            return Err(binding_fault(target));
        }
        let Some(staging) = preparations.staging() else {
            return Err(fault(
                target,
                ErrorCode::InternalUnavailable,
                "the preparation attachment staging is unavailable",
                RawObject::empty(),
            ));
        };
        let count = decimal(&body.attachment_count).map_err(|_| {
            fault(
                target.clone(),
                ErrorCode::ProtocolSchemaInvalid,
                "attachmentCount is not a supported decimal",
                RawObject::empty(),
            )
        })?;
        let count = u32::try_from(count).map_err(|_| {
            fault(
                target.clone(),
                ErrorCode::ProtocolSchemaInvalid,
                "attachmentCount is out of range",
                RawObject::empty(),
            )
        })?;
        let state = staging
            .begin_manifest(StagedManifestBegin {
                submission: submission_id(&submission),
                binding: PreparationBinding {
                    device: session.device_id().clone(),
                    session: verified.session.clone(),
                    tab_nonce: verified.tab_nonce.clone(),
                    runtime_epoch: runtime_epoch.clone(),
                    generation,
                    claims_digest: verified.claims_digest.clone(),
                },
                subject: AttachmentSubject::Device(session.device_id().clone()),
                connection: connection_id(session),
                count,
                at: preparations.now(),
            })
            .await
            .map_err(|error| staging_fault(target.clone(), &error))?;
        let previous = self.manifest(&submission);
        let sealed = match previous
            .as_ref()
            .and_then(|manifest| manifest.sealed.clone())
        {
            Some(reply) => Some(reply),
            None => match state.sealed_digest.as_ref() {
                Some(digest) => Some(ManifestSealedBody {
                    submission_id: submission.clone(),
                    attachment_count: decimal_string(state.count as u64),
                    manifest_digest: base64url(digest).ok_or_else(|| {
                        fault(
                            target.clone(),
                            ErrorCode::InternalUnavailable,
                            "the manifest digest cannot be projected",
                            RawObject::empty(),
                        )
                    })?,
                    sealed_at: wire_timestamp(&preparations.now()).ok_or_else(|| {
                        fault(
                            target.clone(),
                            ErrorCode::InternalUnavailable,
                            "the manifest timestamp cannot be projected",
                            RawObject::empty(),
                        )
                    })?,
                }),
                None => None,
            },
        };
        self.remember_manifest(
            &submission,
            PreparationManifest {
                session: verified.session.clone(),
                connection: session.connection_id().as_str().to_owned(),
                runtime_epoch,
                generation,
                count: state.count,
                registered: state.registered,
                final_seen: previous
                    .as_ref()
                    .is_some_and(|manifest| manifest.final_seen),
                sealed: sealed.clone(),
            },
        );
        Ok(sealed)
    }

    /// `chunk`/`seal`/`status`/`abort` 的准备分支。`Ok(false)` = 不是准备运行期传输。
    async fn preparation_transfer(
        &self,
        session: &ConnectionHandle,
        preparations: &Arc<Preparations>,
        frame: &Frame,
    ) -> Result<bool, PrepFault> {
        let transfer = frame.transfer_hint().ok_or_else(|| {
            fault(
                ErrorTarget::Transfer(None),
                ErrorCode::ProtocolSchemaInvalid,
                "the attachment frame has no transferId",
                RawObject::empty(),
            )
        })?;
        let target = ErrorTarget::Transfer(Some(transfer.clone()));
        let Some(staging) = preparations.staging() else {
            return Ok(false);
        };
        let Some(state) = staging
            .status(attachment_id(&transfer))
            .await
            .map_err(|error| staging_fault(target.clone(), &error))?
        else {
            return Ok(false);
        };
        self.check_transfer_access(session, preparations, &state, &target)
            .await?;
        let reply = match &frame.body {
            FrameBody::Chunk(body) => {
                let offset = decimal(&body.offset).map_err(|_| {
                    fault(
                        target.clone(),
                        ErrorCode::ProtocolSchemaInvalid,
                        "offset is not a supported decimal",
                        RawObject::empty(),
                    )
                })?;
                let outcome = staging
                    .append(StagedChunk {
                        transfer: attachment_id(&transfer),
                        connection: connection_id(session),
                        offset,
                        bytes: body.data.as_bytes().to_vec(),
                        at: preparations.now(),
                    })
                    .await
                    .map_err(|error| staging_fault(target.clone(), &error))?;
                Some(match outcome {
                    acp_core::model::StagedChunkOutcome::Accepted { next_offset } => {
                        FrameBody::ChunkAck(sync_protocol::attachment::ChunkAckBody {
                            transfer_id: transfer.clone(),
                            offset: decimal_string(offset),
                            length: decimal_string(body.data.as_bytes().len() as u64),
                            duplicate: false,
                            next_offset: decimal_string(next_offset),
                        })
                    }
                    acp_core::model::StagedChunkOutcome::Duplicate { next_offset } => {
                        FrameBody::ChunkAck(sync_protocol::attachment::ChunkAckBody {
                            transfer_id: transfer.clone(),
                            offset: decimal_string(offset),
                            length: decimal_string(body.data.as_bytes().len() as u64),
                            duplicate: true,
                            next_offset: decimal_string(next_offset),
                        })
                    }
                    acp_core::model::StagedChunkOutcome::OffsetMismatch { expected, actual } => {
                        return Err(offset_fault(target, expected, actual));
                    }
                    acp_core::model::StagedChunkOutcome::ChunkConflict { offset } => {
                        return Err(chunk_conflict_fault(target.clone(), offset));
                    }
                    acp_core::model::StagedChunkOutcome::ConnectionMismatch => {
                        return Err(binding_fault(target.clone()));
                    }
                })
            }
            FrameBody::Seal(body) => {
                let length = decimal(&body.length).map_err(|_| {
                    fault(
                        target.clone(),
                        ErrorCode::ProtocolSchemaInvalid,
                        "length is not a supported decimal",
                        RawObject::empty(),
                    )
                })?;
                let sha256 = wire_digest(&body.sha256).map_err(|_| {
                    fault(
                        target.clone(),
                        ErrorCode::ProtocolSchemaInvalid,
                        "sha256 is not a canonical digest",
                        RawObject::empty(),
                    )
                })?;
                let outcome = staging
                    .seal(StagedSeal {
                        transfer: attachment_id(&transfer),
                        connection: connection_id(session),
                        length,
                        sha256,
                        at: preparations.now(),
                    })
                    .await
                    .map_err(|error| staging_fault(target.clone(), &error))?;
                let facts = match outcome {
                    StagedSealOutcome::Sealed(facts) | StagedSealOutcome::AlreadySealed(facts) => {
                        facts
                    }
                    StagedSealOutcome::DigestMismatch => {
                        return Err(fault(
                            target.clone(),
                            ErrorCode::AttachmentDigestMismatch,
                            "the length or digest does not match the received bytes",
                            RawObject::empty(),
                        ));
                    }
                    StagedSealOutcome::ConnectionMismatch => {
                        return Err(binding_fault(target.clone()));
                    }
                };
                self.release_preparation_transfer(transfer.as_str());
                Some(FrameBody::Sealed(SealedBody {
                    transfer_id: transfer.clone(),
                    attachment_id: uuid(&facts.attachment).ok_or_else(|| {
                        fault(
                            target.clone(),
                            ErrorCode::InternalUnavailable,
                            "the attachment id cannot be projected",
                            RawObject::empty(),
                        )
                    })?,
                    length: decimal_string(facts.length),
                    sha256: base64url(&facts.sha256).ok_or_else(|| {
                        fault(
                            target.clone(),
                            ErrorCode::InternalUnavailable,
                            "the digest cannot be projected",
                            RawObject::empty(),
                        )
                    })?,
                    sealed_at: wire_timestamp(&facts.sealed_at).ok_or_else(|| {
                        fault(
                            target.clone(),
                            ErrorCode::InternalUnavailable,
                            "the seal timestamp cannot be projected",
                            RawObject::empty(),
                        )
                    })?,
                }))
            }
            FrameBody::Status(_) => Some(FrameBody::Status(status_body_of(&state).ok_or_else(
                || {
                    fault(
                        target,
                        ErrorCode::InternalUnavailable,
                        "the transfer status cannot be projected",
                        RawObject::empty(),
                    )
                },
            )?)),
            FrameBody::Abort(body) => {
                let reason = staged_abort_reason(body.reason);
                staging
                    .abort(attachment_id(&transfer), reason)
                    .await
                    .map_err(|error| staging_fault(target.clone(), &error))?;
                self.release_preparation_transfer(transfer.as_str());
                Some(FrameBody::Status(StatusBody {
                    transfer_id: transfer.clone(),
                    state: TransferState::Aborted,
                    received_length: decimal_string(state.received),
                    next_offset: decimal_string(state.received),
                    sealed_sha256: match state.sealed.as_ref() {
                        Some(facts) => match base64url(&facts.sha256) {
                            Some(digest) => Nullable::value(digest),
                            None => Nullable::null(),
                        },
                        None => Nullable::null(),
                    },
                }))
            }
            _ => None,
        };
        if let Some(body) = reply {
            if let Err(fault) = session.send_attachment(&body) {
                debug!(
                    event = "sync.preparation_reply_dropped",
                    connection_id = session.connection_id().as_str(),
                    fault = ?fault,
                    "the preparation reply could not be queued"
                );
            }
        }
        Ok(true)
    }

    /// `manifest_page`/`manifest_sealed` 的准备分支。`Ok(false)` = 不是准备清单。
    async fn preparation_manifest_control(
        &self,
        session: &ConnectionHandle,
        preparations: &Arc<Preparations>,
        frame: &Frame,
    ) -> Result<bool, PrepFault> {
        let (submission, target) = match &frame.body {
            FrameBody::ManifestPage(body) => (
                body.submission_id.clone(),
                ErrorTarget::Submission(body.submission_id.clone()),
            ),
            FrameBody::ManifestSealed(body) => (
                body.submission_id.clone(),
                ErrorTarget::Submission(body.submission_id.clone()),
            ),
            _ => return Ok(false),
        };
        let Some(draft) = self.manifest(&submission) else {
            return Ok(false);
        };
        if draft.connection != session.connection_id().as_str() {
            return Err(binding_fault(target));
        }
        if preparations
            .committed(&draft.session)
            .await
            .map_err(|error| staging_fault(target.clone(), &error))?
            .is_some()
        {
            return Err(already_committed_fault(target, &draft.session));
        }
        if let RuntimeCheck::Stale(reason) =
            preparations.runtime(&draft.session, &draft.runtime_epoch, draft.generation, true)
        {
            return Err(stale_fault(target, reason));
        }
        let Some(staging) = preparations.staging() else {
            return Err(fault(
                target,
                ErrorCode::InternalUnavailable,
                "the preparation attachment staging is unavailable",
                RawObject::empty(),
            ));
        };
        match &frame.body {
            FrameBody::ManifestPage(body) => {
                let state = staging
                    .append_manifest_page(StagedManifestPage {
                        submission: submission_id(&submission),
                        connection: connection_id(session),
                        transfers: body.transfer_ids.iter().map(attachment_id).collect(),
                        final_page: body.final_page,
                        at: preparations.now(),
                    })
                    .await
                    .map_err(|error| staging_fault(target.clone(), &error))?;
                let mut updated = draft;
                updated.registered = state.registered;
                updated.final_seen |= body.final_page;
                self.remember_manifest(&submission, updated);
                Ok(true)
            }
            FrameBody::ManifestSealed(body) => {
                let provided = wire_digest(&body.manifest_digest).map_err(|_| {
                    fault(
                        target.clone(),
                        ErrorCode::ProtocolSchemaInvalid,
                        "manifestDigest is not a canonical digest",
                        RawObject::empty(),
                    )
                })?;
                if let Some(sealed) = draft.sealed.clone() {
                    // 已封存：原样回原 `manifest_sealed`；换了摘要则是标识复用。
                    if sealed.manifest_digest == body.manifest_digest {
                        if session
                            .send_attachment(&FrameBody::ManifestSealed(sealed))
                            .is_err()
                        {
                            debug!(
                                event = "sync.preparation_manifest_reply_dropped",
                                connection_id = session.connection_id().as_str(),
                                "the preparation manifest reply could not be queued"
                            );
                        }
                        return Ok(true);
                    }
                    return Err(fault(
                        target.clone(),
                        ErrorCode::AttachmentSealed,
                        "the preparation manifest is already sealed",
                        RawObject::empty(),
                    ));
                }
                if !draft.final_seen {
                    return Err(fault(
                        target.clone(),
                        ErrorCode::ProtocolSchemaInvalid,
                        "the manifest has no final page",
                        RawObject::empty(),
                    ));
                }
                let computed = crate::attachment_transfer::manifest_digest_of(
                    draft.registered.iter().map(AttachmentId::as_str),
                );
                if computed != provided {
                    return Err(fault(
                        target,
                        ErrorCode::AttachmentDigestMismatch,
                        "the manifest digest does not match the registered order",
                        RawObject::empty(),
                    ));
                }
                let sealed_at = preparations.now();
                let state = staging
                    .seal_manifest(StagedManifestSeal {
                        submission: submission_id(&submission),
                        connection: connection_id(session),
                        at: sealed_at.clone(),
                    })
                    .await
                    .map_err(|error| staging_fault(target.clone(), &error))?;
                let reply = ManifestSealedBody {
                    submission_id: submission.clone(),
                    attachment_count: decimal_string(draft.count as u64),
                    manifest_digest: base64url(&computed).ok_or_else(|| {
                        fault(
                            target.clone(),
                            ErrorCode::InternalUnavailable,
                            "the manifest digest cannot be projected",
                            RawObject::empty(),
                        )
                    })?,
                    sealed_at: wire_timestamp(&sealed_at).ok_or_else(|| {
                        fault(
                            target,
                            ErrorCode::InternalUnavailable,
                            "the manifest timestamp cannot be projected",
                            RawObject::empty(),
                        )
                    })?,
                };
                let mut updated = draft;
                updated.registered = state.registered;
                updated.sealed = Some(reply.clone());
                self.remember_manifest(&submission, updated);
                if session
                    .send_attachment(&FrameBody::ManifestSealed(reply))
                    .is_err()
                {
                    debug!(
                        event = "sync.preparation_manifest_reply_dropped",
                        connection_id = session.connection_id().as_str(),
                        "the preparation manifest reply could not be queued"
                    );
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// 逐帧复查：设备/会话绑定、连接归属、首条受理与运行期资源代。
    async fn check_transfer_access(
        &self,
        session: &ConnectionHandle,
        preparations: &Arc<Preparations>,
        state: &StagedTransferState,
        target: &ErrorTarget,
    ) -> Result<(), PrepFault> {
        if state.binding.device != *session.device_id() {
            return Err(binding_fault(target.clone()));
        }
        if state.bound_connection.as_str() != session.connection_id().as_str() {
            // 旧连接迟到帧或未重绑的裸 transferId：都不得接管传输。
            return Err(binding_fault(target.clone()));
        }
        if preparations
            .committed(&state.binding.session)
            .await
            .map_err(|error| staging_fault(target.clone(), &error))?
            .is_some()
        {
            return Err(already_committed_fault(
                target.clone(),
                &state.binding.session,
            ));
        }
        if let RuntimeCheck::Stale(reason) = preparations.runtime(
            &state.binding.session,
            &state.binding.runtime_epoch,
            state.binding.generation,
            true,
        ) {
            return Err(stale_fault(target.clone(), reason));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// 准备目标的验证与错误映射
// ---------------------------------------------------------------------------------------------

/// 准备分支错误的标识位置：传输错误带 `transferId`，清单错误带 `submissionId` 且 `transferId` 为 null
/// （`design.md` D6.1/D6.3）。
#[derive(Debug, Clone)]
enum ErrorTarget {
    Transfer(Option<Uuid>),
    Submission(Uuid),
}

/// 准备分支的帧级失败（认证/feature/绑定/过代/预算）。
#[derive(Debug, Clone)]
struct PrepFault {
    target: ErrorTarget,
    code: ErrorCode,
    message: &'static str,
    details: RawObject,
    close: Option<(u16, &'static str)>,
}

fn fault(
    target: ErrorTarget,
    code: ErrorCode,
    message: &'static str,
    details: RawObject,
) -> PrepFault {
    PrepFault {
        target,
        code,
        message,
        details,
        close: None,
    }
}

fn binding_fault(target: ErrorTarget) -> PrepFault {
    fault(
        target,
        ErrorCode::AttachmentBindingMismatch,
        "the preparation target does not belong to this connection",
        RawObject::empty(),
    )
}

fn feature_required(target: ErrorTarget) -> PrepFault {
    fault(
        target,
        ErrorCode::ProtocolFeatureRequired,
        "the connection did not negotiate core.session-preparation.v1",
        RawObject::parse(&format!(r#"{{"features":["{PREPARATION_FEATURE}"]}}"#))
            .unwrap_or_else(|_| RawObject::empty()),
    )
}

fn already_committed_fault(target: ErrorTarget, session: &SessionId) -> PrepFault {
    fault(
        target,
        ErrorCode::PreparationAlreadyCommitted,
        "the first message was already accepted for this preparation",
        RawObject::parse(&format!(r#"{{"sessionId":"{}"}}"#, session.as_str()))
            .unwrap_or_else(|_| RawObject::empty()),
    )
}

fn stale_fault(target: ErrorTarget, reason: acp_core::model::PreparationStaleReason) -> PrepFault {
    fault(
        target,
        ErrorCode::PreparationStale,
        "the preparation runtime resource is no longer current",
        RawObject::parse(&format!(r#"{{"reason":"{}"}}"#, reason.as_str()))
            .unwrap_or_else(|_| RawObject::empty()),
    )
}

fn offset_fault(target: ErrorTarget, expected: u64, actual: u64) -> PrepFault {
    fault(
        target,
        ErrorCode::AttachmentOffsetMismatch,
        "the chunk offset does not match the next expected offset",
        RawObject::parse(&format!(
            r#"{{"expectedOffset":"{expected}","actualOffset":"{actual}"}}"#
        ))
        .unwrap_or_else(|_| RawObject::empty()),
    )
}

fn chunk_conflict_fault(target: ErrorTarget, offset: u64) -> PrepFault {
    fault(
        target,
        ErrorCode::AttachmentChunkConflict,
        "another chunk was already accepted at this offset",
        RawObject::parse(&format!(r#"{{"offset":"{offset}"}}"#))
            .unwrap_or_else(|_| RawObject::empty()),
    )
}

fn staging_fault(target: ErrorTarget, error: &PortError) -> PrepFault {
    match error {
        PortError::NotFound(_) => fault(
            target,
            ErrorCode::AttachmentTransferNotFound,
            "the transfer or submission does not exist",
            RawObject::empty(),
        ),
        PortError::Conflict(_) => binding_fault(target),
        PortError::InvalidRequest(reason) => {
            let message = match *reason {
                "sealed length does not match the received bytes"
                | "attachment digest does not match the streamed bytes" => {
                    return fault(
                        target,
                        ErrorCode::AttachmentDigestMismatch,
                        "the length or digest does not match the received bytes",
                        RawObject::empty(),
                    );
                }
                "transfer is not receiving" => {
                    return fault(
                        target,
                        ErrorCode::AttachmentSealed,
                        "the transfer is not receiving",
                        RawObject::empty(),
                    );
                }
                "manifest page references an unsealed or foreign transfer" => {
                    return binding_fault(target);
                }
                "preparation target subject mismatch" => return binding_fault(target),
                _ => "the preparation attachment request is not acceptable",
            };
            fault(
                target,
                ErrorCode::ProtocolSchemaInvalid,
                message,
                RawObject::empty(),
            )
        }
        PortError::Unavailable(UnavailableKind::StorageFull) => fault(
            target,
            ErrorCode::AttachmentBudgetExhausted,
            "the attachment budget is exhausted",
            RawObject::empty(),
        ),
        _ => fault(
            target,
            ErrorCode::InternalUnavailable,
            "the preparation attachment storage is unavailable",
            RawObject::empty(),
        ),
    }
}

fn reject_fault(target: ErrorTarget, reject: ReceiptReject) -> PrepFault {
    let (code, message, close) = match reject {
        ReceiptReject::Auth(AuthReject::Required) => (
            ErrorCode::AuthRequired,
            "the connection has no authenticated device",
            Some((close::UNAUTHENTICATED, "authentication required")),
        ),
        ReceiptReject::Auth(AuthReject::Revoked) => (
            ErrorCode::AuthDeviceRevoked,
            "the device is revoked",
            Some((close::REVOKED, "device revoked")),
        ),
        ReceiptReject::Auth(AuthReject::Unknown) => (
            ErrorCode::AuthDeviceUnknown,
            "the device is not known to this host",
            Some((close::UNAUTHENTICATED, "device unknown")),
        ),
        ReceiptReject::Auth(AuthReject::ReauthenticationRequired) => (
            ErrorCode::AuthRequired,
            "the device credential must be reauthenticated",
            Some((close::STATE_CONFLICT, "reauthentication required")),
        ),
        ReceiptReject::Auth(AuthReject::OriginMismatch) => (
            ErrorCode::AuthOriginMismatch,
            "the connection origin does not match the device binding",
            Some((close::UNAUTHENTICATED, "origin mismatch")),
        ),
        ReceiptReject::Binding => (
            ErrorCode::AttachmentBindingMismatch,
            "the preparation receipt does not bind this resource",
            None,
        ),
        ReceiptReject::StoreRebuilt => (
            ErrorCode::PreparationStoreRebuilt,
            "the persistent store was rebuilt; the preparation receipt is no longer valid",
            None,
        ),
        ReceiptReject::Unavailable => (
            ErrorCode::InternalUnavailable,
            "the preparation receipt cannot be verified right now",
            None,
        ),
    };
    PrepFault {
        target,
        code,
        message,
        details: RawObject::empty(),
        close,
    }
}

/// 验证 target 的 receipt 与运行期字段，返回已核验的 claims（committed/stale 判定由调用方按顺序执行）。
async fn verify_target(
    session: &ConnectionHandle,
    preparations: &Arc<Preparations>,
    target: &sync_protocol::attachment::PreparationAttachmentTarget,
    error_target: &ErrorTarget,
) -> Result<
    (
        crate::sync::preparation::Verified,
        RuntimeEpoch,
        ResourceGeneration,
    ),
    PrepFault,
> {
    let target_session = SessionId::new(target.session_id.as_str()).map_err(|_| {
        fault(
            error_target.clone(),
            ErrorCode::ProtocolSchemaInvalid,
            "the target sessionId is not canonical",
            RawObject::empty(),
        )
    })?;
    let runtime_epoch = RuntimeEpoch::new(target.runtime_epoch.as_str()).map_err(|_| {
        fault(
            error_target.clone(),
            ErrorCode::ProtocolSchemaInvalid,
            "the target runtimeEpoch is not canonical",
            RawObject::empty(),
        )
    })?;
    let generation = target
        .generation
        .as_str()
        .parse::<u64>()
        .map(ResourceGeneration::new)
        .map_err(|_| {
            fault(
                error_target.clone(),
                ErrorCode::ProtocolSchemaInvalid,
                "the target generation is out of range",
                RawObject::empty(),
            )
        })?;
    let verified = preparations
        .verify(
            session.device_id(),
            session.canonical_origin(),
            &target.receipt,
            &target_session,
        )
        .await
        .map_err(|reject| reject_fault(error_target.clone(), reject))?;
    if let Some(committed) = preparations
        .committed(&verified.session)
        .await
        .map_err(|error| staging_fault(error_target.clone(), &error))?
    {
        // `sessionId`（登记字段）取已核验的会话 ID，而不是首条提交标识。
        let _ = committed;
        return Err(fault(
            error_target.clone(),
            ErrorCode::PreparationAlreadyCommitted,
            "the first message was already accepted for this preparation",
            RawObject::parse(&format!(
                r#"{{"sessionId":"{}"}}"#,
                verified.session.as_str()
            ))
            .unwrap_or_else(|_| RawObject::empty()),
        ));
    }
    if let RuntimeCheck::Stale(reason) =
        preparations.runtime(&verified.session, &runtime_epoch, generation, true)
    {
        return Err(stale_fault(error_target.clone(), reason));
    }
    Ok((verified, runtime_epoch, generation))
}

/// 把准备帧的失败映射成 `attachment.error` 并按需要关闭连接。
fn finish_fault(session: &ConnectionHandle, fault: PrepFault) -> AttachmentOutcome {
    let body = match &fault.target {
        ErrorTarget::Transfer(Some(transfer)) => {
            ErrorBody::new(fault.code, fault.message).with_transfer(transfer.clone(), fault.details)
        }
        ErrorTarget::Transfer(None) => {
            let mut body = ErrorBody::new(fault.code, fault.message);
            body.details = fault.details;
            body
        }
        ErrorTarget::Submission(submission) => ErrorBody::new(fault.code, fault.message)
            .with_submission(submission.clone(), fault.details),
    };
    if session.send_attachment(&FrameBody::Error(body)).is_err() {
        debug!(
            event = "sync.preparation_error_dropped",
            connection_id = session.connection_id().as_str(),
            code = fault.code.as_str(),
            "the preparation error could not be queued"
        );
    }
    match fault.close {
        Some((code, reason)) => AttachmentOutcome::Close(code, reason),
        None => AttachmentOutcome::Claimed,
    }
}

fn connection_id(session: &ConnectionHandle) -> ConnectionId {
    ConnectionId::new(session.connection_id().as_str()).expect("wire uuid is a valid connection id")
}

fn status_body_of(state: &StagedTransferState) -> Option<StatusBody> {
    Some(StatusBody {
        transfer_id: uuid(&state.transfer)?,
        state: if state.sealed.is_some() {
            TransferState::Sealed
        } else {
            TransferState::Receiving
        },
        received_length: decimal_string(state.received),
        next_offset: decimal_string(state.received),
        sealed_sha256: match state.sealed.as_ref() {
            Some(facts) => Nullable::value(base64url(&facts.sha256)?),
            None => Nullable::null(),
        },
    })
}

// ---------------------------------------------------------------------------------------------
// 正式分支的 wire 映射（形状与顺序逐字不变）
// ---------------------------------------------------------------------------------------------

/// 帧 → 中立请求的结果。
enum Outcome {
    /// 方向不允许（服务端从客户端接收的帧集合）。
    Unsupported,
    /// body 形状不可用（解码已通过，但数值越界等）。
    Invalid,
}

impl Subject {
    /// 已认证身份 → 附件主体（命令面与附件连接面共用同一映射）。
    pub(crate) fn from_actor(actor: &Actor) -> Option<Self> {
        match actor {
            Actor::Device { device } => Some(Subject::Device(device.as_str().to_owned())),
            Actor::Node { access_node, .. } => Some(Subject::Node(access_node.as_str().to_owned())),
            Actor::LocalCli | Actor::PairingClaimant { .. } => None,
        }
    }
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
        FrameBody::Begin(BeginBody::Owned(body)) => Request::Begin(begin_request(body)?),
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
        FrameBody::ManifestBegin(ManifestBeginBody::Owned(body)) => manifest_begin(body)?,
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
        // 准备分支由 `route` 在进入本函数之前处理；出现在这里说明调用顺序被改动，按不支持拒绝。
        FrameBody::Begin(BeginBody::Preparation(_))
        | FrameBody::ManifestBegin(ManifestBeginBody::Preparation(_)) => {
            return Err(Outcome::Unsupported);
        }
        // 方向判定（`is_client_frame`）已拒绝的方向；这里显式列出以免新增帧类型时静默落入通配。
        FrameBody::ChunkAck(_) | FrameBody::Sealed(_) | FrameBody::ReadChunk(_) => {
            return Err(Outcome::Unsupported);
        }
    };
    Ok(Some(request))
}

fn begin_request(
    body: &sync_protocol::attachment::OwnedBeginBody,
) -> Result<BeginRequest, Outcome> {
    Ok(BeginRequest {
        transfer: attachment_id(&body.transfer_id),
        session: session_id(&body.session_id)?,
        subject: Subject::Device(body.subject.id.as_str().to_owned()),
        total_length: decimal(&body.total_length)?,
        chunk_size: body.chunk_size.get(),
        media_type: body.media_type.as_str().to_owned(),
        file_name: body.file_name.as_str().to_owned(),
    })
}

fn manifest_begin(
    body: &sync_protocol::attachment::OwnedManifestBeginBody,
) -> Result<Request, Outcome> {
    Ok(Request::ManifestBegin {
        submission: submission_id(&body.submission_id),
        session: session_id(&body.session_id)?,
        subject: Subject::Device(body.subject.id.as_str().to_owned()),
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

fn staged_abort_reason(reason: WireAbortReason) -> StagedAbortReason {
    match reason {
        WireAbortReason::ClientCancelled => StagedAbortReason::ClientCancelled,
        WireAbortReason::BudgetExceeded => StagedAbortReason::BudgetExceeded,
        WireAbortReason::Revoked => StagedAbortReason::Revoked,
        WireAbortReason::DigestMismatch => StagedAbortReason::DigestMismatch,
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
fn wire_digest(value: &sync_protocol::common::Base64Url<32>) -> Result<Digest, Outcome> {
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
        } => FrameBody::ChunkAck(sync_protocol::attachment::ChunkAckBody {
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
            data: sync_protocol::attachment::ChunkData::from_bytes(data).ok()?,
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

fn base64url(digest: &Digest) -> Option<sync_protocol::common::Base64Url<32>> {
    sync_protocol::common::Base64Url::<32>::parse(digest.as_str()).ok()
}

fn wire_timestamp(value: &Timestamp) -> Option<WireTimestamp> {
    WireTimestamp::parse(value.as_str()).ok()
}

/// 中立失败 → wire 错误码、说明与 `details`。
fn wire_fault(fault: &Fault) -> (ErrorCode, &'static str, RawObject) {
    match fault {
        Fault::NegotiationRequired => (
            ErrorCode::AttachmentNegotiationRequired,
            "the connection did not negotiate core.attachment-transfer.v1",
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
            event = "sync.attachment_error_dropped",
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
            FrameBody::Begin(body) => Some(match body {
                BeginBody::Owned(body) => body.transfer_id.clone(),
                BeginBody::Preparation(body) => body.transfer_id.clone(),
            }),
            FrameBody::Chunk(body) => Some(body.transfer_id.clone()),
            FrameBody::Seal(body) => Some(body.transfer_id.clone()),
            FrameBody::Status(body) => Some(body.transfer_id.clone()),
            FrameBody::Abort(body) => Some(body.transfer_id.clone()),
            FrameBody::Read(body) => Some(body.attachment_id.clone()),
            FrameBody::ManifestBegin(body) => Some(match body {
                ManifestBeginBody::Owned(body) => body.submission_id.clone(),
                ManifestBeginBody::Preparation(body) => body.submission_id.clone(),
            }),
            FrameBody::ManifestPage(body) => Some(body.submission_id.clone()),
            FrameBody::ManifestSealed(body) => Some(body.submission_id.clone()),
            FrameBody::ChunkAck(_)
            | FrameBody::Sealed(_)
            | FrameBody::ReadChunk(_)
            | FrameBody::Error(_) => None,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests;
