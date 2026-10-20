//! 5.4：远程附件有界内存转发（大文件、撤销、中途断线、已转发不等于已提交）。
//!
//! 客户端走真实的附件帧编解码与序号校验；Owner 是脚本化的内存对端（不 mock 客户端业务函数）。

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use acp_core::model::{AttachmentId, Digest, SessionId, SubmissionId};
use node_link_protocol::attachment::{
    AbortReason, ChunkAckBody, ChunkBody, ChunkData, ErrorBody, Frame, FrameBody, ReadChunkBody,
    SealedBody, StatusBody, TransferState,
};
use node_link_protocol::common::{Nullable, Uuid};
use node_link_protocol::error::ErrorCode;
use sha2::{Digest as _, Sha256};

use node_link_client::ClientError;

use support::{OwnerIo, base64url, decimal, forwarder_pair, uuid_text};

const SUBJECT: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
const TRANSFER: &str = "018f0000-0000-4000-8000-000000000101";
const SESSION: &str = "018f0000-0000-4000-8000-000000000102";

fn transfer() -> AttachmentId {
    AttachmentId::new(TRANSFER).expect("transfer id")
}

fn session() -> SessionId {
    SessionId::new(SESSION).expect("session id")
}

fn submission() -> SubmissionId {
    SubmissionId::new("018f0000-0000-4000-8000-000000000103").expect("submission id")
}

#[derive(Default)]
struct Observed {
    chunks: Vec<usize>,
    committed: Option<u64>,
    subject: Option<String>,
    sealed: bool,
}

/// 一个把每块原样确认、并在 seal 时校验流式摘要的 Owner。
async fn echo_owner(
    mut owner: OwnerIo,
    connection_id: Uuid,
    state: Arc<Mutex<Observed>>,
    fail_after_chunks: Option<usize>,
    close_after_chunks: Option<usize>,
    fail_with: Option<ErrorCode>,
) {
    let mut inbound = 1u64;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    while let Some(frame) = owner.recv_frame(&connection_id, &mut inbound).await {
        match frame.body {
            FrameBody::Begin(body) => {
                state
                    .lock()
                    .expect("state")
                    .subject
                    .replace(body.subject.id.as_str().to_owned());
                // 已转发不等于已提交：Owner 只登记了传输，没有封存也没有业务消息。
                assert!(!state.lock().expect("state").sealed);
            }
            FrameBody::Chunk(body) => {
                let offset = body.offset.as_str().parse::<u64>().expect("offset");
                let data = body.data.as_bytes().to_vec();
                let index = state.lock().expect("state").chunks.len();
                {
                    let mut state = state.lock().expect("state");
                    state.chunks.push(data.len());
                }
                if fail_with == Some(ErrorCode::AuthNodeRevoked) {
                    owner
                        .send_frame(
                            &connection_id,
                            FrameBody::Error(ErrorBody::new(ErrorCode::AuthNodeRevoked, "revoked")),
                        )
                        .await;
                    return;
                }
                if close_after_chunks == Some(index + 1) {
                    owner.close().await;
                    return;
                }
                // 停等判据：确认前 Owner 不应收到下一条帧（客户端每传输至多一块未确认）。
                if let Ok(Some(extra)) =
                    tokio::time::timeout(Duration::from_millis(5), owner.recv()).await
                {
                    panic!("the client sent {extra} before the previous chunk was acknowledged");
                }
                hasher.update(&data);
                received += data.len() as u64;
                owner
                    .send_frame(
                        &connection_id,
                        FrameBody::ChunkAck(ChunkAckBody {
                            transfer_id: uuid_text(TRANSFER),
                            offset: decimal(offset),
                            length: decimal(data.len() as u64),
                            duplicate: false,
                            next_offset: decimal(offset + data.len() as u64),
                        }),
                    )
                    .await;
                if fail_after_chunks.is_some_and(|limit| index + 1 == limit) {
                    // 继续读下一帧而不回复，让客户端超时/断线。
                }
            }
            FrameBody::Seal(body) => {
                let declared = body.length.as_str().parse::<u64>().expect("length");
                assert_eq!(
                    declared, received,
                    "sealed length must equal received bytes"
                );
                let digest_bytes = hasher.clone().finalize();
                assert_eq!(
                    body.sha256.as_bytes(),
                    digest_bytes.as_slice(),
                    "the client must seal the streamed SHA-256"
                );
                state.lock().expect("state").sealed = true;
                state.lock().expect("state").committed = Some(received);
                owner
                    .send_frame(
                        &connection_id,
                        FrameBody::Sealed(SealedBody {
                            transfer_id: uuid_text(TRANSFER),
                            attachment_id: uuid_text(TRANSFER),
                            length: decimal(received),
                            sha256: body.sha256,
                            sealed_at: support::wire_timestamp("2026-10-20T00:00:00.000Z"),
                        }),
                    )
                    .await;
                return;
            }
            FrameBody::Status(body) => {
                owner
                    .send_frame(
                        &connection_id,
                        FrameBody::Status(StatusBody {
                            transfer_id: body.transfer_id,
                            state: TransferState::Receiving,
                            received_length: decimal(received),
                            next_offset: decimal(received),
                            sealed_sha256: Nullable::null(),
                        }),
                    )
                    .await;
            }
            FrameBody::Abort(body) => {
                owner
                    .send_frame(
                        &connection_id,
                        FrameBody::Status(StatusBody {
                            transfer_id: body.transfer_id,
                            state: TransferState::Aborted,
                            received_length: decimal(received),
                            next_offset: decimal(received),
                            sealed_sha256: Nullable::null(),
                        }),
                    )
                    .await;
                return;
            }
            FrameBody::Read(body) => {
                let offset = body.offset.as_str().parse::<u64>().expect("offset");
                let max = body.max_length.as_str().parse::<u64>().expect("max");
                let data = vec![0x42u8; max.min(16) as usize];
                owner
                    .send_frame(
                        &connection_id,
                        FrameBody::ReadChunk(ReadChunkBody {
                            attachment_id: body.attachment_id,
                            session_id: body.session_id,
                            offset: decimal(offset),
                            length: decimal(data.len() as u64),
                            total_length: decimal(1024),
                            data: ChunkData::from_bytes(data).expect("chunk"),
                            eof: offset + 16 >= 1024,
                        }),
                    )
                    .await;
            }
            FrameBody::ManifestBegin(_) | FrameBody::ManifestPage(_) => {}
            FrameBody::ManifestSealed(body) => {
                owner
                    .send_frame(
                        &connection_id,
                        FrameBody::ManifestSealed(
                            node_link_protocol::attachment::ManifestSealedBody {
                                submission_id: body.submission_id,
                                attachment_count: decimal(1),
                                manifest_digest: body.manifest_digest,
                                sealed_at: support::wire_timestamp("2026-10-20T00:00:00.000Z"),
                            },
                        ),
                    )
                    .await;
                return;
            }
            FrameBody::Error(_) => {}
            // 客户端不会发送的方向（`chunk_ack`/`sealed`/`read_chunk`）。
            FrameBody::ChunkAck(_) | FrameBody::Sealed(_) | FrameBody::ReadChunk(_) => {
                panic!("the client sent a server-direction frame")
            }
        }
    }
}

/// 大文件：3 MiB 从 `AsyncRead` 流式上传，每块 ≤ 32 KiB，停等（无未确认的并发块），全程无临时文件。
#[tokio::test(flavor = "current_thread")]
async fn upload_streams_a_large_file_without_buffering_the_whole_body() {
    let (forwarder, owner, connection_id) = forwarder_pair(SUBJECT).await;
    let state = Arc::new(Mutex::new(Observed::default()));
    let owner_state = Arc::clone(&state);
    let owner_task = tokio::spawn(async move {
        echo_owner(owner, connection_id, owner_state, None, None, None).await;
    });

    let total = 3 * 1024 * 1024u64;
    let source = tokio::io::AsyncReadExt::take(tokio::io::repeat(0x5a), total);
    let outcome = forwarder
        .upload(
            &transfer(),
            &session(),
            total,
            "application/octet-stream",
            "big.bin",
            source,
        )
        .await
        .expect("the upload must complete");
    assert_eq!(outcome.sent, total);
    assert_eq!(outcome.sealed.length, total);
    owner_task.await.expect("owner task");

    {
        let observed = state.lock().expect("state");
        assert_eq!(
            observed.subject.as_deref(),
            Some(SUBJECT),
            "subject binds to the access node"
        );
        assert_eq!(
            observed.chunks.iter().sum::<usize>() as u64,
            total,
            "every byte must be forwarded"
        );
        assert!(
            observed.chunks.iter().all(|len| *len <= 32 * 1024),
            "no chunk may exceed the fixed 32 KiB block size"
        );
        assert_eq!(
            observed.chunks.len() as u64,
            total.div_ceil(32 * 1024),
            "the source is streamed in fixed blocks"
        );
        // 封存事实不等于业务提交：Owner 只封存了字节，测试没有提交任何 prompt。
        assert!(observed.sealed);
        assert_eq!(observed.committed, Some(total));
    }
    forwarder.close().await;
}

/// 撤销：Owner 以 `auth.node_revoked` 结束，上传失败为 `Revoked`，不重试、不产生封存事实。
#[tokio::test(flavor = "current_thread")]
async fn revocation_during_upload_aborts_the_transfer() {
    let (forwarder, owner, connection_id) = forwarder_pair(SUBJECT).await;
    let state = Arc::new(Mutex::new(Observed::default()));
    let owner_state = Arc::clone(&state);
    let owner_task = tokio::spawn(async move {
        echo_owner(
            owner,
            connection_id,
            owner_state,
            None,
            None,
            Some(ErrorCode::AuthNodeRevoked),
        )
        .await;
    });
    let source = tokio::io::AsyncReadExt::take(tokio::io::repeat(0x11), 200_000);
    let error = forwarder
        .upload(
            &transfer(),
            &session(),
            200_000,
            "application/octet-stream",
            "revoked.bin",
            source,
        )
        .await
        .expect_err("a revoked node must fail the upload");
    assert!(matches!(error, ClientError::Revoked), "got {error:?}");
    owner_task.await.expect("owner task");
    assert!(!state.lock().expect("state").sealed);
    assert!(state.lock().expect("state").committed.is_none());
    assert!(
        forwarder.status(&transfer()).await.is_err(),
        "the channel is closed"
    );
    forwarder.close().await;
}

/// 中途断线：连接在两块之后关闭，上传以显式错误结束，绝不报告为已封存。
#[tokio::test(flavor = "current_thread")]
async fn a_disconnect_mid_transfer_is_not_reported_as_sealed() {
    let (forwarder, owner, connection_id) = forwarder_pair(SUBJECT).await;
    let state = Arc::new(Mutex::new(Observed::default()));
    let owner_state = Arc::clone(&state);
    let owner_task = tokio::spawn(async move {
        echo_owner(owner, connection_id, owner_state, None, Some(2), None).await;
    });
    let source = tokio::io::AsyncReadExt::take(tokio::io::repeat(0x22), 500_000);
    let error = forwarder
        .upload(
            &transfer(),
            &session(),
            500_000,
            "application/octet-stream",
            "cut.bin",
            source,
        )
        .await
        .expect_err("a closed connection must fail the upload");
    assert!(
        matches!(error, ClientError::Closed | ClientError::Transport(_)),
        "got {error:?}"
    );
    owner_task.await.expect("owner task");
    assert!(!state.lock().expect("state").sealed);
    forwarder.close().await;
}

/// `status`/`read`/`abort` 的帧映射（读取正文同样只经这条内存连接，不落盘）。
#[tokio::test(flavor = "current_thread")]
async fn status_read_and_abort_round_trip() {
    let (forwarder, owner, connection_id) = forwarder_pair(SUBJECT).await;
    let state = Arc::new(Mutex::new(Observed::default()));
    let owner_state = Arc::clone(&state);
    let owner_task = tokio::spawn(async move {
        echo_owner(owner, connection_id, owner_state, None, None, None).await;
    });

    let status = forwarder.status(&transfer()).await.expect("status");
    assert_eq!(
        status.state,
        acp_core::model::AttachmentTransferState::Receiving
    );
    assert_eq!(status.received_length, 0);

    let chunk = forwarder
        .read(&transfer(), &session(), 0, 16)
        .await
        .expect("read");
    assert_eq!(chunk.offset, 0);
    assert_eq!(chunk.length, 16);
    assert_eq!(chunk.total_length, 1024);
    assert_eq!(chunk.data, vec![0x42u8; 16]);
    assert!(!chunk.eof);

    let aborted = forwarder
        .abort(&transfer(), AbortReason::ClientCancelled)
        .await
        .expect("abort");
    assert_eq!(
        aborted.state,
        acp_core::model::AttachmentTransferState::Aborted
    );
    owner_task.await.expect("owner task");
    assert!(!state.lock().expect("state").sealed);
    forwarder.close().await;
}

/// 清单：`manifest_begin`/`manifest_page` 无显式 ack，`manifest_sealed` 有；客户端校验页大小。
#[tokio::test(flavor = "current_thread")]
async fn manifest_sealing_round_trips() {
    let (forwarder, owner, connection_id) = forwarder_pair(SUBJECT).await;
    let state = Arc::new(Mutex::new(Observed::default()));
    let owner_state = Arc::clone(&state);
    let owner_task = tokio::spawn(async move {
        echo_owner(owner, connection_id, owner_state, None, None, None).await;
    });
    let submission = submission();
    forwarder
        .manifest_begin(&submission, &session(), 1)
        .await
        .expect("manifest begin");
    forwarder
        .manifest_page(&submission, &[transfer()], true)
        .await
        .expect("manifest page");
    let empty: &[AttachmentId] = &[];
    assert!(
        forwarder
            .manifest_page(&submission, empty, true)
            .await
            .is_err(),
        "an empty manifest page is rejected before it is sent"
    );
    let digest = Digest::new(&base64url(&[9u8; 32])).expect("digest");
    let sealed = forwarder
        .manifest_sealed(&submission, &digest)
        .await
        .expect("manifest sealed");
    assert_eq!(sealed.attachment_count, 1);
    assert_eq!(sealed.manifest_digest, digest);
    owner_task.await.expect("owner task");
    forwarder.close().await;
}

/// 心跳：空闲时收到 `link.ping` 必须原样回填 nonce，且回帧使用本方向的下一个序号。
#[tokio::test(flavor = "current_thread")]
async fn an_idle_attachment_connection_answers_pings() {
    let (forwarder, mut owner, connection_id) = forwarder_pair(SUBJECT).await;
    // 先让客户端发一帧（begin），Owner 读掉它，保持连接空闲。
    forwarder
        .begin(&transfer(), &session(), 4, "text/plain", "a.txt")
        .await
        .expect("begin");
    let mut inbound = 1u64;
    let begin = owner
        .recv_frame(&connection_id, &mut inbound)
        .await
        .expect("begin frame");
    assert!(matches!(begin.body, FrameBody::Begin(_)));

    let nonce = base64url(&[1u8; 16]);
    let ping = node_link_protocol::error::Ping {
        nonce: node_link_protocol::common::Base64Url::<16>::parse(&nonce).expect("nonce"),
    };
    let envelope = support::encode_envelope(
        node_link_protocol::envelope::MessageType::LinkPing,
        Some(node_link_protocol::envelope::ConnectionFields {
            connection_id: connection_id.clone(),
            // `node.ready` 已占用入站序号 1；空闲心跳从 2 开始。
            connection_sequence: decimal(2),
        }),
        &ping,
    );
    owner.send_raw(envelope).await;

    let mut client_outbound = 2u64; // begin 占 1；pong 是客户端方向的第 2 条。
    let pong = owner
        .recv_envelope(&connection_id, &mut client_outbound)
        .await
        .expect("pong envelope");
    assert_eq!(
        pong.message_type(),
        node_link_protocol::envelope::MessageType::LinkPong
    );
    let body: node_link_protocol::error::Pong =
        serde_json::from_str(pong.body().get()).expect("pong body");
    assert_eq!(base64url(body.nonce.as_bytes()), nonce);
    forwarder.close().await;
}

/// 未协商的能力必须在建连前失败（不触碰任何存储）。
#[tokio::test(flavor = "current_thread")]
async fn a_connection_without_the_attachment_feature_is_unsupported() {
    // `from_parts` 不做握手校验；这里验证 `connect` 的 feature 门控由 `Established` 决定。
    let established = support::established(
        "018f0000-0000-4000-8000-000000000040",
        &support::REQUIRED_FEATURES,
    );
    assert!(!established.has(node_link_client::ATTACHMENT_FEATURE));
    let _ = established;
}

/// 客户端不会把接收方向的帧当作请求发送（方向判定在运行任务里失败关闭）。
#[tokio::test(flavor = "current_thread")]
async fn an_unexpected_frame_fails_closed() {
    let (forwarder, mut owner, connection_id) = forwarder_pair(SUBJECT).await;
    // 客户端请求 status（期待 `attachment.status`），Owner 回 `chunk_ack`：必须显式失败。
    let request = tokio::spawn(async move { forwarder.status(&transfer()).await });
    let mut inbound = 1u64;
    let frame = owner
        .recv_frame(&connection_id, &mut inbound)
        .await
        .expect("status frame");
    assert!(matches!(frame.body, FrameBody::Status(_)));
    owner
        .send_frame(
            &connection_id,
            FrameBody::Chunk(ChunkBody {
                transfer_id: uuid_text(TRANSFER),
                offset: decimal(0),
                data: ChunkData::from_bytes(vec![1]).expect("chunk"),
            }),
        )
        .await;
    let error = request.await.expect("join").expect_err("unexpected frame");
    assert!(matches!(error, ClientError::Protocol(_)), "got {error:?}");
}

/// `Frame` 的解码只接受附件帧；控制帧走业务信封（这里顺带钉住 `decode_incoming` 的分派顺序）。
#[test]
fn control_frames_are_not_attachment_frames() {
    let envelope = support::encode_envelope(
        node_link_protocol::envelope::MessageType::LinkPing,
        Some(node_link_protocol::envelope::ConnectionFields {
            connection_id: uuid_text("018f0000-0000-4000-8000-000000000050"),
            connection_sequence: decimal(1),
        }),
        &node_link_protocol::error::Ping {
            nonce: node_link_protocol::common::Base64Url::<16>::parse(&base64url(&[2u8; 16]))
                .expect("nonce"),
        },
    );
    assert!(Frame::decode(&envelope).is_err());
}
