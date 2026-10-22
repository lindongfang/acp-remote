//! 附件准备目标（`design.md` D6.1–D6.4）的协议级用例：双 feature 门控、receipt/device 绑定、
//! committed/stale/store_rebuilt、跨连接重绑与旧连接迟到帧、封存/清单幂等与 `submissionId` 错误定位。
//!
//! 正式（owned）分支的形状与门控由本模块既有的两条用例覆盖；这里只测准备分支，且全部依赖都是窄替身
//! （准备暂存与信任面来自 `preparation::tests` 的同一套固定装置）。

use std::net::IpAddr;
use std::sync::Arc;

use acp_core::model::{
    AttachmentId, DeviceId, InteractionId, MessageId, NodeId, OriginEpoch, PairingId, PortError,
    RequestId, SessionId, SubmissionId, Timestamp, TurnId,
};
use acp_core::ports::{
    AttachmentChunk, AttachmentRead, AttachmentTransferStore, ChunkOutcome, Clock, IdGenerator,
    SubmissionBegin, SubmissionPage, SubmissionProgress, SubmissionSeal, TransferBegin, TransferChunk,
    TransferSeal,
};
use acp_core::model::{AttachmentSubmission, AttachmentTransfer, SealedAttachment};
use sync_protocol::attachment::{
    BeginBody, DeviceSubject, FixedChunkSize, Frame, FrameBody, ManifestBeginBody,
    PreparationAttachmentTarget, PreparationBeginBody, PreparationManifestBeginBody,
    PreparationTargetKind,
};
use sync_protocol::common::{Base64Url, DecimalString, NonEmptyText, PositiveDecimalString, Uuid};
use sync_protocol::error::ErrorCode;

use super::*;
use crate::sync::conn::limits::{SessionLimits, SyncConfig};
use crate::sync::preparation::tests::{
    canonical_origin, committed_record, fixture, handle, ready_preparation, sign_receipt, Fixture,
    AGENT, DEVICE, RUNTIME, TAB,
};
use crate::sync::conn::registry::Outbound;

// ---------------------------------------------------------------------------------------------
// 正式引擎的不可达替身（准备用例不应触碰正式存储）
// ---------------------------------------------------------------------------------------------

struct UnreachableStore;

fn unreachable_store() -> PortError {
    panic!("preparation frames must not touch the owned attachment store")
}

#[async_trait::async_trait]
impl AttachmentTransferStore for UnreachableStore {
    async fn begin_transfer(&self, _write: TransferBegin) -> Result<AttachmentTransfer, PortError> {
        Err(unreachable_store())
    }
    async fn append_chunk(&self, _write: TransferChunk) -> Result<ChunkOutcome, PortError> {
        Err(unreachable_store())
    }
    async fn seal_transfer(&self, _write: TransferSeal) -> Result<SealedAttachment, PortError> {
        Err(unreachable_store())
    }
    async fn transfer(
        &self,
        _transfer: &AttachmentId,
    ) -> Result<Option<AttachmentTransfer>, PortError> {
        Err(unreachable_store())
    }
    async fn abort_transfer(
        &self,
        _transfer: &AttachmentId,
        _at: Timestamp,
    ) -> Result<AttachmentTransfer, PortError> {
        Err(unreachable_store())
    }
    async fn begin_submission(
        &self,
        _write: SubmissionBegin,
    ) -> Result<SubmissionProgress, PortError> {
        Err(unreachable_store())
    }
    async fn append_submission_page(
        &self,
        _write: SubmissionPage,
    ) -> Result<SubmissionProgress, PortError> {
        Err(unreachable_store())
    }
    async fn finalize_submission(
        &self,
        _write: SubmissionSeal,
    ) -> Result<AttachmentSubmission, PortError> {
        Err(unreachable_store())
    }
    async fn submission(
        &self,
        _submission: &SubmissionId,
    ) -> Result<Option<AttachmentSubmission>, PortError> {
        Err(unreachable_store())
    }
    async fn read_chunk(&self, request: &AttachmentRead) -> Result<AttachmentChunk, PortError> {
        Err(PortError::NotFound(acp_core::model::EntityRef::Attachment(
            request.attachment.clone(),
        )))
    }
    async fn prune_transfers(&self, _at: Timestamp, _limit: u32) -> Result<u32, PortError> {
        Err(unreachable_store())
    }
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::new("2026-01-01T00:00:00.000Z").expect("timestamp")
    }
}

struct TestIds;

fn fresh() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl IdGenerator for TestIds {
    fn session_id(&self) -> SessionId {
        SessionId::new(&fresh()).expect("session id")
    }
    fn turn_id(&self) -> TurnId {
        TurnId::new(&fresh()).expect("turn id")
    }
    fn interaction_id(&self) -> InteractionId {
        InteractionId::new(&fresh()).expect("interaction id")
    }
    fn pairing_id(&self) -> PairingId {
        PairingId::new(&fresh()).expect("pairing id")
    }
    fn message_id(&self) -> MessageId {
        MessageId::new(&fresh()).expect("message id")
    }
    fn origin_epoch(&self) -> OriginEpoch {
        OriginEpoch::new(&fresh()).expect("origin epoch")
    }
    fn request_id(&self) -> RequestId {
        RequestId::new(&fresh()).expect("request id")
    }
}

fn uuid(text: &str) -> Uuid {
    Uuid::parse(text).expect("uuid")
}

fn service(fixture: &Fixture) -> Arc<AttachmentService> {
    AttachmentService::from_engine_with_preparation(
        Engine::new(Arc::new(UnreachableStore), Arc::new(FixedClock)),
        fixture.preparations.clone(),
    )
}

fn features() -> Vec<String> {
    vec![ATTACHMENT_FEATURE.to_owned(), PREPARATION_FEATURE.to_owned()]
}

fn attachment_features() -> Vec<String> {
    vec![ATTACHMENT_FEATURE.to_owned()]
}

fn target(
    fixture: &Fixture,
    session: &SessionId,
    receipt: sync_protocol::common::PreparationReceipt,
) -> PreparationAttachmentTarget {
    let _ = fixture;
    PreparationAttachmentTarget {
        kind: PreparationTargetKind,
        session_id: uuid(session.as_str()),
        receipt,
        runtime_epoch: uuid(RUNTIME),
        generation: PositiveDecimalString::parse("1").expect("generation"),
    }
}

fn begin_frame(
    handle: &ConnectionHandle,
    transfer: &Uuid,
    target: PreparationAttachmentTarget,
    subject: &Uuid,
    total: u64,
    file_name: &str,
) -> Frame {
    Frame::new(
        uuid(&fresh()),
        handle.connection_id().clone(),
        DecimalString::parse("1").expect("sequence"),
        FrameBody::Begin(BeginBody::Preparation(PreparationBeginBody {
            transfer_id: transfer.clone(),
            target,
            subject: DeviceSubject::new(subject.clone()),
            total_length: DecimalString::parse(&total.to_string()).expect("length"),
            chunk_size: FixedChunkSize::new(32768).expect("chunk size"),
            media_type: NonEmptyText::<255>::parse("text/plain").expect("media type"),
            file_name: NonEmptyText::<255>::parse(file_name).expect("file name"),
        })),
    )
}

fn chunk_frame(handle: &ConnectionHandle, transfer: &Uuid, offset: u64, length: usize) -> Frame {
    Frame::new(
        uuid(&fresh()),
        handle.connection_id().clone(),
        DecimalString::parse("1").expect("sequence"),
        FrameBody::Chunk(sync_protocol::attachment::ChunkBody {
            transfer_id: transfer.clone(),
            offset: DecimalString::parse(&offset.to_string()).expect("offset"),
            data: sync_protocol::attachment::ChunkData::from_bytes(vec![7u8; length])
                .expect("chunk data"),
        }),
    )
}

fn seal_frame(handle: &ConnectionHandle, transfer: &Uuid, length: u64, sha: Base64Url<32>) -> Frame {
    Frame::new(
        uuid(&fresh()),
        handle.connection_id().clone(),
        DecimalString::parse("1").expect("sequence"),
        FrameBody::Seal(sync_protocol::attachment::SealBody {
            transfer_id: transfer.clone(),
            length: DecimalString::parse(&length.to_string()).expect("length"),
            sha256: sha,
        }),
    )
}

fn digest_of(bytes: &[u8]) -> Base64Url<32> {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
    Base64Url::<32>::parse(&encoded).expect("digest")
}

fn manifest_begin_frame(
    handle: &ConnectionHandle,
    submission: &Uuid,
    target: PreparationAttachmentTarget,
    subject: &Uuid,
    count: u64,
) -> Frame {
    Frame::new(
        uuid(&fresh()),
        handle.connection_id().clone(),
        DecimalString::parse("1").expect("sequence"),
        FrameBody::ManifestBegin(ManifestBeginBody::Preparation(
            PreparationManifestBeginBody {
                submission_id: submission.clone(),
                target,
                subject: DeviceSubject::new(subject.clone()),
                attachment_count: DecimalString::parse(&count.to_string()).expect("count"),
            },
        )),
    )
}

fn drain(receiver: &mut tokio::sync::mpsc::Receiver<Outbound>) -> Vec<Frame> {
    let mut frames = Vec::new();
    while let Ok(outbound) = receiver.try_recv() {
        frames.push(Frame::decode(&outbound.text).expect("attachment frame"));
    }
    frames
}

fn error_of(frames: &[Frame]) -> &sync_protocol::attachment::ErrorBody {
    frames
        .iter()
        .find_map(|frame| match &frame.body {
            FrameBody::Error(body) => Some(body),
            _ => None,
        })
        .expect("an attachment.error frame")
}

fn subject_of(device: &str) -> Uuid {
    uuid(device)
}

// ---------------------------------------------------------------------------------------------
// 用例
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn preparation_begin_without_the_feature_is_rejected_before_staging() {
    let fixture = fixture();
    let (session, _view) = ready_preparation(&fixture).await;
    let (handle, mut receiver) = handle(DEVICE);
    let transfer = uuid(&fresh());
    let frame = begin_frame(
        &handle,
        &transfer,
        target(&fixture, &session, sign_receipt(&fixture, &session, TAB, AGENT)),
        &subject_of(DEVICE),
        4,
        "note.txt",
    );
    let outcome = service(&fixture)
        .route(&handle, &attachment_features(), &frame)
        .await;
    assert_eq!(outcome, AttachmentOutcome::Claimed);
    let frames = drain(&mut receiver);
    let error = error_of(&frames);
    assert_eq!(error.code, ErrorCode::ProtocolFeatureRequired);
    assert_eq!(error.transfer_id.as_ref().map(Uuid::as_str), Some(transfer.as_str()));
    assert_eq!(fixture.staging.transfer_count(), 0);
}

#[tokio::test]
async fn preparation_begin_requires_the_authenticated_device_and_binding() {
    let fixture = fixture();
    let (session, _view) = ready_preparation(&fixture).await;
    let receipt = sign_receipt(&fixture, &session, TAB, AGENT);
    let (handle, mut receiver) = handle(DEVICE);
    // subject 声明与连接身份不一致：binding_mismatch，且不触碰暂存。
    let frame = begin_frame(
        &handle,
        &uuid(&fresh()),
        target(&fixture, &session, receipt),
        &subject_of("9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a"),
        4,
        "note.txt",
    );
    service(&fixture)
        .route(&handle, &features(), &frame)
        .await;
    let frames = drain(&mut receiver);
    assert_eq!(error_of(&frames).code, ErrorCode::AttachmentBindingMismatch);
    assert_eq!(fixture.staging.transfer_count(), 0);

    // 另一台设备使用为 DEVICE 签发的 receipt：签名有效但设备绑定不符。
    let receipt = sign_receipt(&fixture, &session, TAB, AGENT);
    let (other, mut other_receiver) = handle("9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a");
    let frame = begin_frame(
        &other,
        &uuid(&fresh()),
        target(&fixture, &session, receipt),
        &subject_of("9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a"),
        4,
        "note.txt",
    );
    service(&fixture).route(&other, &features(), &frame).await;
    let frames = drain(&mut other_receiver);
    assert_eq!(error_of(&frames).code, ErrorCode::AttachmentBindingMismatch);
    assert_eq!(fixture.staging.transfer_count(), 0);
}

#[tokio::test]
async fn preparation_begin_after_commit_reports_session_id() {
    let fixture = fixture();
    let (session, _view) = ready_preparation(&fixture).await;
    fixture.first.set(committed_record(&session));
    let (handle, mut receiver) = handle(DEVICE);
    let transfer = uuid(&fresh());
    let frame = begin_frame(
        &handle,
        &transfer,
        target(&fixture, &session, sign_receipt(&fixture, &session, TAB, AGENT)),
        &subject_of(DEVICE),
        4,
        "note.txt",
    );
    service(&fixture)
        .route(&handle, &features(), &frame)
        .await;
    let frames = drain(&mut receiver);
    let error = error_of(&frames);
    assert_eq!(error.code, ErrorCode::PreparationAlreadyCommitted);
    assert!(!error.retryable);
    assert_eq!(
        error.transfer_id.as_ref().map(Uuid::as_str),
        Some(transfer.as_str())
    );
    let details = error.details.get();
    assert!(details.contains(session.as_str()), "details must locate the session: {details}");
    assert_eq!(fixture.staging.transfer_count(), 0);
}

#[tokio::test]
async fn preparation_begin_stale_on_runtime_epoch_mismatch() {
    let fixture = fixture();
    let (session, _view) = ready_preparation(&fixture).await;
    let (handle, mut receiver) = handle(DEVICE);
    let mut target = target(
        &fixture,
        &session,
        sign_receipt(&fixture, &session, TAB, AGENT),
    );
    target.runtime_epoch = uuid("11111111-2222-4333-8444-555555555555");
    let frame = begin_frame(
        &handle,
        &uuid(&fresh()),
        target,
        &subject_of(DEVICE),
        4,
        "note.txt",
    );
    service(&fixture)
        .route(&handle, &features(), &frame)
        .await;
    let frames = drain(&mut receiver);
    let error = error_of(&frames);
    assert_eq!(error.code, ErrorCode::PreparationStale);
    assert!(error.retryable);
    assert!(error.details.get().contains("runtime_epoch_mismatch"));
    assert_eq!(fixture.staging.transfer_count(), 0);
}

#[tokio::test]
async fn chunk_upload_and_cross_connection_rebind() {
    let fixture = fixture();
    let (session, _view) = ready_preparation(&fixture).await;
    let receipt = sign_receipt(&fixture, &session, TAB, AGENT);
    let transfer = uuid(&fresh());
    let (first_connection, mut first_receiver) = handle(DEVICE);
    let first = begin_frame(
        &first_connection,
        &transfer,
        target(&fixture, &session, receipt.clone()),
        &subject_of(DEVICE),
        8,
        "note.txt",
    );
    service(&fixture)
        .route(&first_connection, &features(), &first)
        .await;
    assert!(drain(&mut first_receiver).is_empty(), "begin has no reply");
    assert_eq!(fixture.staging.transfer_count(), 1);

    let chunk = chunk_frame(&first_connection, &transfer, 0, 4);
    service(&fixture)
        .route(&first_connection, &features(), &chunk)
        .await;
    let frames = drain(&mut first_receiver);
    let ack = frames
        .iter()
        .find_map(|frame| match &frame.body {
            FrameBody::ChunkAck(ack) => Some(ack),
            _ => None,
        })
        .expect("chunk ack");
    assert_eq!(ack.next_offset.as_str(), "4");

    // 新附件连接重发完整 target 的 begin（同 transferId、同声明）才能接管。
    let (second_connection, mut second_receiver) = handle(DEVICE);
    let rebind = begin_frame(
        &second_connection,
        &transfer,
        target(&fixture, &session, receipt.clone()),
        &subject_of(DEVICE),
        8,
        "note.txt",
    );
    service(&fixture)
        .route(&second_connection, &features(), &rebind)
        .await;
    assert!(drain(&mut second_receiver).is_empty());

    // 旧连接迟到帧被拒（不修改传输）。
    let late = chunk_frame(&first_connection, &transfer, 4, 4);
    service(&fixture)
        .route(&first_connection, &features(), &late)
        .await;
    let frames = drain(&mut first_receiver);
    assert_eq!(error_of(&frames).code, ErrorCode::AttachmentBindingMismatch);

    // 新连接从原偏移继续。
    let chunk = chunk_frame(&second_connection, &transfer, 4, 4);
    service(&fixture)
        .route(&second_connection, &features(), &chunk)
        .await;
    let frames = drain(&mut second_receiver);
    let ack = frames
        .iter()
        .find_map(|frame| match &frame.body {
            FrameBody::ChunkAck(ack) => Some(ack),
            _ => None,
        })
        .expect("chunk ack");
    assert_eq!(ack.next_offset.as_str(), "8");
}

#[tokio::test]
async fn seal_is_idempotent_and_status_reads_the_staged_state() {
    let fixture = fixture();
    let (session, _view) = ready_preparation(&fixture).await;
    let transfer = uuid(&fresh());
    let (handle, mut receiver) = handle(DEVICE);
    let begin = begin_frame(
        &handle,
        &transfer,
        target(&fixture, &session, sign_receipt(&fixture, &session, TAB, AGENT)),
        &subject_of(DEVICE),
        4,
        "note.txt",
    );
    let service = service(&fixture);
    service.route(&handle, &features(), &begin).await;
    let chunk = chunk_frame(&handle, &transfer, 0, 4);
    service.route(&handle, &features(), &chunk).await;
    let _ = drain(&mut receiver);

    let seal = seal_frame(&handle, &transfer, 4, digest_of(&[7u8; 4]));
    service.route(&handle, &features(), &seal).await;
    let frames = drain(&mut receiver);
    let sealed = frames
        .iter()
        .find_map(|frame| match &frame.body {
            FrameBody::Sealed(sealed) => Some(sealed),
            _ => None,
        })
        .expect("sealed");
    assert_eq!(sealed.length.as_str(), "4");

    // 重放同一封存：幂等返回同一事实（不报错、不改变字节）。
    service.route(&handle, &features(), &seal).await;
    let frames = drain(&mut receiver);
    let repeat = frames
        .iter()
        .find_map(|frame| match &frame.body {
            FrameBody::Sealed(sealed) => Some(sealed),
            _ => None,
        })
        .expect("sealed replay");
    assert_eq!(repeat.sha256.clone(), sealed.sha256.clone());

    let status = Frame::new(
        uuid(&fresh()),
        handle.connection_id().clone(),
        DecimalString::parse("1").expect("sequence"),
        FrameBody::Status(sync_protocol::attachment::StatusBody {
            transfer_id: transfer.clone(),
            state: sync_protocol::attachment::TransferState::Receiving,
            received_length: DecimalString::parse("0").expect("length"),
            next_offset: DecimalString::parse("0").expect("offset"),
            sealed_sha256: sync_protocol::common::Nullable::null(),
        }),
    );
    service.route(&handle, &features(), &status).await;
    let frames = drain(&mut receiver);
    let status = frames
        .iter()
        .find_map(|frame| match &frame.body {
            FrameBody::Status(status) => Some(status),
            _ => None,
        })
        .expect("status");
    assert_eq!(status.state, sync_protocol::attachment::TransferState::Sealed);
}

#[tokio::test]
async fn manifest_errors_carry_the_submission_id() {
    let fixture = fixture();
    let (session, _view) = ready_preparation(&fixture).await;
    let submission = uuid(&fresh());
    let (handle, mut receiver) = handle(DEVICE);
    let frame = manifest_begin_frame(
        &handle,
        &submission,
        target(&fixture, &session, sign_receipt(&fixture, &session, TAB, AGENT)),
        &subject_of(DEVICE),
        1,
    );
    service(&fixture)
        .route(&handle, &attachment_features(), &frame)
        .await;
    let frames = drain(&mut receiver);
    let error = error_of(&frames);
    assert_eq!(error.code, ErrorCode::ProtocolFeatureRequired);
    assert!(error.transfer_id.is_null(), "manifest errors must not carry transferId");
    assert_eq!(
        error.submission_id.as_ref().map(Uuid::as_str),
        Some(submission.as_str())
    );
}

#[tokio::test]
async fn preparation_read_is_not_extended_to_staged_transfers() {
    let fixture = fixture();
    let (session, _view) = ready_preparation(&fixture).await;
    let transfer = uuid(&fresh());
    let (handle, mut receiver) = handle(DEVICE);
    let begin = begin_frame(
        &handle,
        &transfer,
        target(&fixture, &session, sign_receipt(&fixture, &session, TAB, AGENT)),
        &subject_of(DEVICE),
        4,
        "note.txt",
    );
    service(&fixture)
        .route(&handle, &features(), &begin)
        .await;
    let _ = drain(&mut receiver);
    let read = Frame::new(
        uuid(&fresh()),
        handle.connection_id().clone(),
        DecimalString::parse("1").expect("sequence"),
        FrameBody::Read(sync_protocol::attachment::ReadBody {
            attachment_id: transfer,
            session_id: uuid(session.as_str()),
            offset: DecimalString::parse("0").expect("offset"),
            max_length: DecimalString::parse("4").expect("max"),
        }),
    );
    service(&fixture).route(&handle, &features(), &read).await;
    // `read` 仍走正式读取面：尚未转正的附件是 `attachment.transfer_not_found`（不泄露暂存内容）。
    let frames = drain(&mut receiver);
    assert_eq!(error_of(&frames).code, ErrorCode::AttachmentTransferNotFound);
}

// ---------------------------------------------------------------------------------------------
// 正式（owned）分支保持原样：未协商附件 feature 时仍以 4400 关闭
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn formal_negotiation_gate_is_unchanged() {
    let device = DeviceId::new(DEVICE).expect("device");
    let (handle, mut receiver) = handle(DEVICE);
    let _ = device;
    let frame = Frame::new(
        uuid(&fresh()),
        handle.connection_id().clone(),
        DecimalString::parse("1").expect("sequence"),
        FrameBody::Begin(BeginBody::Owned(sync_protocol::attachment::OwnedBeginBody {
            transfer_id: uuid(&fresh()),
            session_id: uuid(&fresh()),
            subject: DeviceSubject::new(subject_of(DEVICE)),
            total_length: DecimalString::parse("4").expect("length"),
            chunk_size: FixedChunkSize::new(32768).expect("chunk size"),
            media_type: NonEmptyText::<255>::parse("text/plain").expect("media type"),
            file_name: NonEmptyText::<255>::parse("note.txt").expect("file name"),
        })),
    );
    let outcome = service(&fixture())
        .route(&handle, &[], &frame)
        .await;
    assert_eq!(
        outcome,
        AttachmentOutcome::Close(
            close::PROTOCOL,
            "attachment feature was not negotiated"
        )
    );
    let frames = drain(&mut receiver);
    assert_eq!(
        error_of(&frames).code,
        ErrorCode::AttachmentNegotiationRequired
    );
}

// 未使用的导入保持显式（用例可能按平台裁剪）。
#[allow(dead_code)]
fn _uses(handle: &ConnectionHandle) -> (IpAddr, NodeId, SessionLimits) {
    (
        handle.client_ip(),
        handle.host_id().clone(),
        SessionLimits::negotiate(&SyncConfig::default()),
    )
}

#[allow(dead_code)]
fn _idle(clock: &FixedClock) -> Timestamp {
    clock.now()
}

#[allow(dead_code)]
fn _canonical() -> identity_auth::CanonicalOrigin {
    canonical_origin()
}
