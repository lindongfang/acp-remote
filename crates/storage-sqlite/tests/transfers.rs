//! §5.8/§6 第 22 条/§7.6：分块附件传输——偏移、幂等、流式摘要、内容寻址与 24 小时清理。

mod support;

use acp_core::model::{
    AgentId, AgentRef, AttachmentId, AttachmentSubject, AttachmentTransferState, ConflictKind,
    DeviceId, Digest, EntityRef, OriginEpoch, PortError, SessionId, Timestamp, UnavailableKind,
};
use acp_core::ports::{
    AttachmentRead, AttachmentStore, AttachmentTransferStore, ChunkOutcome, NewSession,
    OwnedCommit, SessionStore, StateChange, TransferBegin, TransferChunk, TransferSeal,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const ORIGIN_EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
const DEVICE: &str = "55555555-5555-4555-8555-555555555555";

fn at(hours: u32) -> Timestamp {
    let day = 18 + hours / 24;
    let hour = hours % 24;
    Timestamp::new(&format!("2026-09-{day:02}T{hour:02}:00:00.000Z")).expect("timestamp")
}

fn transfer_id(n: u64) -> AttachmentId {
    AttachmentId::new(&format!("00000000-0000-4000-8000-{n:012}")).expect("transfer id")
}

fn session_id(n: u64) -> SessionId {
    SessionId::new(&format!("00000000-0000-4000-8000-{n:012}")).expect("session id")
}

fn subject() -> AttachmentSubject {
    AttachmentSubject::Device(DeviceId::new(DEVICE).expect("device id"))
}

async fn store(dir: &std::path::Path) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at(0))
        .await
        .expect("open store")
}

async fn create_session(store: &SqliteStore) -> SessionId {
    store
        .commit(OwnedCommit {
            session: None,
            at: at(0),
            expected_version: None,
            state: Some(StateChange::Create(NewSession {
                title: Some("transfers".to_owned()),
                agent: AgentRef::try_new(
                    AgentId::new("probe-agent").expect("agent id"),
                    "Probe Agent",
                )
                .expect("agent ref"),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(OriginEpoch::new(ORIGIN_EPOCH).expect("epoch")),
            input: None,
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id")
}

fn begin(session: &SessionId, transfer: &AttachmentId, total_length: u64) -> TransferBegin {
    TransferBegin {
        transfer: transfer.clone(),
        session: session.clone(),
        subject: subject(),
        total_length,
        media_type: "application/octet-stream".to_owned(),
        file_name: "payload.bin".to_owned(),
        at: at(1),
    }
}

/// 确定性的块内容：不构造整文件，调用方只为单块分配内存。
fn chunk_at(offset: u64, length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| ((offset + index as u64) % 251) as u8)
        .collect()
}

/// 以 32 KiB 步长流式计算摘要（与实现同一算法，但不把正文放进内存）。
fn streamed_digest(total: u64) -> Digest {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    let mut offset = 0_u64;
    while offset < total {
        let length = usize::try_from((total - offset).min(32_768)).expect("chunk length");
        hasher.update(chunk_at(offset, length));
        offset += length as u64;
    }
    Digest::new(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize()))
        .expect("digest")
}

fn chunk(transfer: &AttachmentId, offset: u64, data: Vec<u8>) -> TransferChunk {
    TransferChunk {
        transfer: transfer.clone(),
        offset,
        data,
        at: at(1),
    }
}

fn seal(transfer: &AttachmentId, length: u64, sha256: Digest, hour: u32) -> TransferSeal {
    TransferSeal {
        transfer: transfer.clone(),
        length,
        sha256,
        at: at(hour),
    }
}

fn temporary(dir: &std::path::Path, transfer: &AttachmentId) -> std::path::PathBuf {
    dir.join("attachments")
        .join(".transfers")
        .join(format!("{}.part", transfer.as_str()))
}

/// 期望偏移：重复同块幂等、同位置不同内容拒绝、非期望偏移拒绝；单块上限固定为 32 KiB。
#[tokio::test]
async fn chunks_follow_the_expected_offset_and_reject_conflicts() {
    let dir = temp_dir("transfer-chunks");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    let transfer = transfer_id(1);
    let first = chunk_at(0, 32_768);

    store
        .begin_transfer(begin(&session, &transfer, 40_000))
        .await
        .expect("begin");

    let ack = store
        .append_chunk(chunk(&transfer, 0, first.clone()))
        .await
        .expect("first chunk");
    assert_eq!(
        ack,
        ChunkOutcome {
            next_offset: 32_768,
            duplicate: false
        }
    );

    // 同偏移同内容 → 原确认，不重复写入。
    let repeat = store
        .append_chunk(chunk(&transfer, 0, first.clone()))
        .await
        .expect("duplicate chunk");
    assert_eq!(
        repeat,
        ChunkOutcome {
            next_offset: 32_768,
            duplicate: true
        }
    );
    assert_eq!(
        std::fs::metadata(temporary(&dir, &transfer))
            .expect("temp file")
            .len(),
        32_768,
        "重复块不得再写一遍"
    );

    // 同偏移不同内容 → 冲突。
    let mut different = first.clone();
    different[0] ^= 0xff;
    let error = store
        .append_chunk(chunk(&transfer, 0, different))
        .await
        .expect_err("same offset, different bytes");
    assert!(matches!(
        error,
        PortError::Conflict(ConflictKind::AlreadyExists)
    ));

    // 越过期望偏移 → 可重试的请求错误；不得把空洞后的字节写进去。
    let error = store
        .append_chunk(chunk(&transfer, 32_769, chunk_at(32_769, 10)))
        .await
        .expect_err("offset ahead of the expected one");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    // 单块 >32 KiB 不具备 API 表达（合同固定常量）。
    let error = store
        .append_chunk(chunk(&transfer, 32_768, vec![0; 32_769]))
        .await
        .expect_err("oversized chunk");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    // 超出声明总长 → 拒绝，不回退已确认长度。
    let error = store
        .append_chunk(chunk(&transfer, 32_768, chunk_at(32_768, 32_768)))
        .await
        .expect_err("chunk exceeds total length");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    let second = store
        .append_chunk(chunk(&transfer, 32_768, chunk_at(32_768, 7_232)))
        .await
        .expect("second chunk");
    assert_eq!(second.next_offset, 40_000);

    let state = store
        .transfer(&transfer)
        .await
        .expect("read transfer")
        .expect("transfer");
    assert_eq!(state.received_length, 40_000);
    assert_eq!(state.state, AttachmentTransferState::Receiving);
    assert_eq!(state.chunk_size, 32_768);
    assert_eq!(
        std::fs::metadata(temporary(&dir, &transfer))
            .expect("temp file")
            .len(),
        40_000
    );

    store.close().await;
}

/// 封存：流式摘要与长度必须同时正确；内容寻址去重；读取始终按块返回并校验会话归属。
#[tokio::test]
async fn seal_streams_the_digest_and_reads_are_session_scoped() {
    let dir = temp_dir("transfer-seal");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    let other = session_id(2);
    store
        .commit(OwnedCommit {
            session: None,
            at: at(0),
            expected_version: None,
            state: Some(StateChange::Create(NewSession {
                title: Some("other".to_owned()),
                agent: AgentRef::try_new(
                    AgentId::new("probe-agent").expect("agent id"),
                    "Probe Agent",
                )
                .expect("agent ref"),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(OriginEpoch::new(ORIGIN_EPOCH).expect("epoch")),
            input: None,
        })
        .await
        .expect("second session");
    assert_ne!(session, other);

    let transfer = transfer_id(3);
    let total = 40_000_u64;
    store
        .begin_transfer(begin(&session, &transfer, total))
        .await
        .expect("begin");
    let mut offset = 0;
    while offset < total {
        let length = usize::try_from((total - offset).min(32_768)).expect("length");
        store
            .append_chunk(chunk(&transfer, offset, chunk_at(offset, length)))
            .await
            .expect("append");
        offset += length as u64;
    }
    let digest = streamed_digest(total);

    // 长度不符不产生可引用附件。
    let error = store
        .seal_transfer(seal(&transfer, total - 1, digest.clone(), 2))
        .await
        .expect_err("length mismatch");
    assert!(matches!(error, PortError::InvalidRequest(_)));
    // 摘要不符同样拒绝，且传输保持 receiving。
    let error = store
        .seal_transfer(seal(&transfer, total, streamed_digest(total - 1), 2))
        .await
        .expect_err("digest mismatch");
    assert!(matches!(error, PortError::InvalidRequest(_)));
    let state = store.transfer(&transfer).await.expect("read").expect("row");
    assert_eq!(state.state, AttachmentTransferState::Receiving);
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        0
    );
    pool.close().await;

    let sealed = store
        .seal_transfer(seal(&transfer, total, digest.clone(), 2))
        .await
        .expect("seal");
    assert_eq!(sealed.length, total);
    assert_eq!(sealed.sha256, digest);
    let state = store.transfer(&transfer).await.expect("read").expect("row");
    assert_eq!(state.state, AttachmentTransferState::Sealed);
    assert_eq!(
        state.sealed.as_ref().map(|sealed| &sealed.attachment),
        Some(&sealed.attachment)
    );

    // 同一内容再次封存 → 同一附件标识（内容寻址只存一份字节）。
    let again = transfer_id(4);
    store
        .begin_transfer(begin(&session, &again, total))
        .await
        .expect("begin again");
    let mut offset = 0;
    while offset < total {
        let length = usize::try_from((total - offset).min(32_768)).expect("length");
        store
            .append_chunk(chunk(&again, offset, chunk_at(offset, length)))
            .await
            .expect("append again");
        offset += length as u64;
    }
    let deduped = store
        .seal_transfer(seal(&again, total, digest.clone(), 2))
        .await
        .expect("seal again");
    assert_eq!(deduped.attachment, sealed.attachment);
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        1
    );
    pool.close().await;

    // 读取需要会话归属：封存本身不建立会话绑定（prompt 受理时才绑定）。这里用旧路径的 `link`
    // 建立归属，随后验证跨会话读取仍被拒绝。
    store
        .link(
            &session,
            &sealed.attachment,
            acp_core::model::AttachmentGeneration::new(1),
            at(3),
        )
        .await
        .expect("link");

    // 读取仍按块返回：`maxLength` 被 32 KiB 常量截断。
    let small = store
        .read_chunk(&AttachmentRead {
            attachment: sealed.attachment.clone(),
            session: session.clone(),
            offset: 0,
            max_length: 1_000,
        })
        .await
        .expect("read small");
    assert_eq!(small.data.len(), 1_000);
    assert!(!small.eof);
    assert_eq!(small.total_length, total);

    let capped = store
        .read_chunk(&AttachmentRead {
            attachment: sealed.attachment.clone(),
            session: session.clone(),
            offset: 0,
            max_length: u64::MAX,
        })
        .await
        .expect("read capped");
    assert_eq!(capped.data.len(), 32_768, "read_chunk 不得超过固定块大小");

    let last = store
        .read_chunk(&AttachmentRead {
            attachment: sealed.attachment.clone(),
            session: session.clone(),
            offset: 32_768,
            max_length: u64::MAX,
        })
        .await
        .expect("read last");
    assert_eq!(last.data.len(), 7_232);
    assert!(last.eof);

    // 读取需要会话归属：封存本身不建立会话绑定（prompt 受理时才绑定）。
    let denied = store
        .read_chunk(&AttachmentRead {
            attachment: sealed.attachment.clone(),
            session: other,
            offset: 0,
            max_length: 1024,
        })
        .await
        .expect_err("cross-session read");
    assert!(matches!(denied, PortError::NotFound(_)));

    store.close().await;
}

/// 大文件只经分块 API 处理：单块缓冲有界、摘要流式，进程不需要整文件 `Vec`。
#[tokio::test]
async fn large_transfers_use_bounded_buffers() {
    let dir = temp_dir("transfer-large");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    let transfer = transfer_id(5);
    let total = 8 * 1024 * 1024_u64;
    store
        .begin_transfer(begin(&session, &transfer, total))
        .await
        .expect("begin");

    let mut offset = 0;
    while offset < total {
        let length = usize::try_from((total - offset).min(32_768)).expect("length");
        let ack = store
            .append_chunk(chunk(&transfer, offset, chunk_at(offset, length)))
            .await
            .expect("append");
        assert_eq!(ack.next_offset, offset + length as u64);
        offset += length as u64;
    }
    let sealed = store
        .seal_transfer(seal(&transfer, total, streamed_digest(total), 2))
        .await
        .expect("seal");
    store
        .link(
            &session,
            &sealed.attachment,
            acp_core::model::AttachmentGeneration::new(1),
            at(3),
        )
        .await
        .expect("link");

    let mut read = 0_u64;
    while read < total {
        let chunk = store
            .read_chunk(&AttachmentRead {
                attachment: sealed.attachment.clone(),
                session: session.clone(),
                offset: read,
                max_length: u64::MAX,
            })
            .await
            .expect("read");
        assert!(chunk.data.len() <= 32_768, "单次读取的缓冲不得超过块大小");
        read += chunk.data.len() as u64;
    }
    assert_eq!(read, total);
    store.close().await;
}

/// 失败关闭：缺文件、越界读取与撤销都不会留下可引用半成品。
#[tokio::test]
async fn missing_payloads_and_aborts_fail_closed() {
    let dir = temp_dir("transfer-errors");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    let transfer = transfer_id(6);
    let total = 16_u64;
    store
        .begin_transfer(begin(&session, &transfer, total))
        .await
        .expect("begin");
    store
        .append_chunk(chunk(&transfer, 0, chunk_at(0, total as usize)))
        .await
        .expect("append");

    // 临时文件被外部删除：封存返回 Corrupt，不创建附件行。
    std::fs::remove_file(temporary(&dir, &transfer)).expect("remove temp");
    let error = store
        .seal_transfer(seal(&transfer, total, streamed_digest(total), 2))
        .await
        .expect_err("missing payload");
    assert!(matches!(error, PortError::Corrupt(_)));
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        0
    );
    pool.close().await;

    // 撤销在事务内中止仍在接收的传输，并删除临时文件。
    let aborted = store.abort_transfer(&transfer, at(3)).await.expect("abort");
    assert_eq!(aborted.state, AttachmentTransferState::Aborted);
    let error = store
        .append_chunk(chunk(&transfer, total, vec![1]))
        .await
        .expect_err("aborted transfer rejects chunks");
    assert!(matches!(error, PortError::InvalidRequest(_)));
    assert!(!temporary(&dir, &transfer).exists());

    // 未声明总长之外的读取偏移被拒绝。
    let sealed_transfer = transfer_id(7);
    store
        .begin_transfer(begin(&session, &sealed_transfer, 4))
        .await
        .expect("begin sealed");
    store
        .append_chunk(chunk(&sealed_transfer, 0, chunk_at(0, 4)))
        .await
        .expect("append");
    let sealed = store
        .seal_transfer(seal(&sealed_transfer, 4, streamed_digest(4), 2))
        .await
        .expect("seal");
    // 封存不等于绑定；先绑定再读。
    store
        .commit_with_submission(
            acceptance(&session, "11111111-1111-4111-8111-111111111111"),
            None,
        )
        .await
        .expect("accept");
    // 手工建立 link（没有清单时由调用方直接 link 的旧路径仍可用）。
    store
        .link(
            &session,
            &sealed.attachment,
            acp_core::model::AttachmentGeneration::new(1),
            at(3),
        )
        .await
        .expect("link");
    let error = store
        .read_chunk(&AttachmentRead {
            attachment: sealed.attachment,
            session,
            offset: 5,
            max_length: 1,
        })
        .await
        .expect_err("offset past end");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    store.close().await;
}

fn acceptance(session: &SessionId, request: &str) -> OwnedCommit {
    OwnedCommit {
        session: Some(session.clone()),
        at: at(1),
        expected_version: None,
        state: None,
        turns: Vec::new(),
        events: Vec::new(),
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: Some(acp_core::ports::IdempotencyRecord {
            actor: acp_core::model::Actor::Device {
                device: DeviceId::new(DEVICE).expect("device"),
            },
            request: acp_core::model::RequestId::new(request).expect("request"),
            command: "session.prompt".to_owned(),
            kind: acp_core::model::CommandKind::Mutation,
            session: Some(session.clone()),
            expected_version: None,
            request_fingerprint: Digest::new(&digest_text("acceptance")).expect("digest"),
            accepted_at: at(1),
        }),
        command_terminal: None,
        origin_epoch: None,
        input: None,
    }
}

/// 24 小时闲置窗口：窗口内不清理、窗口外只清理仍在接收且未被清单引用的传输；文件删除在提交之后。
#[tokio::test]
async fn prune_only_expires_idle_receiving_transfers() {
    let dir = temp_dir("transfer-prune");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    let stale = transfer_id(8);
    let fresh = transfer_id(9);
    store
        .begin_transfer(begin(&session, &stale, 4))
        .await
        .expect("begin stale");
    store
        .append_chunk(chunk(&stale, 0, chunk_at(0, 4)))
        .await
        .expect("append stale");
    store
        .begin_transfer(begin(&session, &fresh, 4))
        .await
        .expect("begin fresh");

    // 23 小时内不清理。
    assert_eq!(
        store
            .prune_transfers(at(22), 100)
            .await
            .expect("prune early"),
        0
    );
    assert_eq!(
        store
            .transfer(&stale)
            .await
            .expect("read")
            .expect("row")
            .state,
        AttachmentTransferState::Receiving
    );

    // 超过 24 小时：只过期仍在接收的传输，临时文件在事务提交后删除。
    assert_eq!(store.prune_transfers(at(25), 100).await.expect("prune"), 2);
    assert_eq!(
        store
            .transfer(&stale)
            .await
            .expect("read")
            .expect("row")
            .state,
        AttachmentTransferState::Expired
    );
    assert!(!temporary(&dir, &stale).exists());
    assert!(!temporary(&dir, &fresh).exists());
    let error = store
        .append_chunk(chunk(&stale, 4, vec![5]))
        .await
        .expect_err("expired transfer rejects chunks");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    // 已封存的传输不参与临时清理。
    let sealed_transfer = transfer_id(10);
    store
        .begin_transfer(begin(&session, &sealed_transfer, 4))
        .await
        .expect("begin sealed");
    store
        .append_chunk(chunk(&sealed_transfer, 0, chunk_at(0, 4)))
        .await
        .expect("append sealed");
    store
        .seal_transfer(seal(&sealed_transfer, 4, streamed_digest(4), 26))
        .await
        .expect("seal");
    assert_eq!(
        store
            .prune_transfers(at(50), 100)
            .await
            .expect("prune sealed"),
        0
    );
    assert_eq!(
        store
            .transfer(&sealed_transfer)
            .await
            .expect("read")
            .expect("row")
            .state,
        AttachmentTransferState::Sealed
    );

    store.close().await;
}

/// 清理与 seal 的竞态：任一时序都只留下「已封存」或「已过期」之一，不出现可引用的半成品。
#[tokio::test]
async fn prune_and_seal_race_converges() {
    for round in 0..8_u64 {
        let dir = temp_dir(&format!("transfer-race-{round}"));
        let store = store(&dir).await;
        let session = create_session(&store).await;
        let transfer = transfer_id(20 + round);
        store
            .begin_transfer(begin(&session, &transfer, 4))
            .await
            .expect("begin");
        store
            .append_chunk(chunk(&transfer, 0, chunk_at(0, 4)))
            .await
            .expect("append");
        let digest = streamed_digest(4);
        let prune_store = &store;
        let seal_store = &store;
        let transfer_a = transfer.clone();
        let transfer_b = transfer.clone();
        let (pruned, sealed) = tokio::join!(
            prune_store.prune_transfers(at(25), 100),
            seal_store.seal_transfer(seal(&transfer_b, 4, digest, 1)),
        );
        let pruned = pruned.expect("prune");
        let state = store
            .transfer(&transfer_a)
            .await
            .expect("read")
            .expect("row")
            .state;
        match sealed {
            Ok(_) => {
                assert_eq!(pruned, 0, "封存先赢时不得过期同一条传输");
                assert_eq!(state, AttachmentTransferState::Sealed);
            }
            Err(PortError::InvalidRequest(_)) | Err(PortError::Conflict(_)) => {
                assert_eq!(pruned, 1);
                assert_eq!(state, AttachmentTransferState::Expired);
            }
            Err(other) => panic!("unexpected seal error: {other:?}"),
        }
        let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
        let attachments = scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await;
        assert_eq!(
            attachments,
            if state == AttachmentTransferState::Sealed {
                1
            } else {
                0
            },
            "不得留下可引用的半成品"
        );
        pool.close().await;
        store.close().await;
    }
}

/// 写失败（临时文件缺失）不回退行状态，也不产生可引用的附件；失败可以重试。
#[tokio::test]
async fn io_failures_do_not_mark_a_transfer_sealed() {
    let dir = temp_dir("transfer-io");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    let transfer = transfer_id(40);
    store
        .begin_transfer(begin(&session, &transfer, 2))
        .await
        .expect("begin");
    store
        .append_chunk(chunk(&transfer, 0, chunk_at(0, 2)))
        .await
        .expect("append");
    std::fs::remove_file(temporary(&dir, &transfer)).expect("remove temp");
    let error = store
        .seal_transfer(seal(&transfer, 2, streamed_digest(2), 2))
        .await
        .expect_err("missing file");
    assert!(matches!(error, PortError::Corrupt(_)), "{error:?}");
    assert_eq!(
        store
            .transfer(&transfer)
            .await
            .expect("read")
            .expect("row")
            .state,
        AttachmentTransferState::Receiving
    );
    // 失败后重新开始同 id 的传输是幂等的：不会重置已确认长度（也因此仍缺文件）；新的 id 可以完成。
    let retry = transfer_id(41);
    store
        .begin_transfer(begin(&session, &retry, 2))
        .await
        .expect("begin retry");
    store
        .append_chunk(chunk(&retry, 0, chunk_at(0, 2)))
        .await
        .expect("append retry");
    store
        .seal_transfer(seal(&retry, 2, streamed_digest(2), 2))
        .await
        .expect("seal retry");
    store.close().await;
}

/// `begin` 同一传输重发是幂等的：同参数返回既有进度，不同参数冲突；未知会话/空媒体类型失败关闭。
#[tokio::test]
async fn begin_is_idempotent_and_validates_inputs() {
    let dir = temp_dir("transfer-begin");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    let transfer = transfer_id(50);
    let first = store
        .begin_transfer(begin(&session, &transfer, 10))
        .await
        .expect("begin");
    assert_eq!(first.received_length, 0);

    let again = store
        .begin_transfer(begin(&session, &transfer, 10))
        .await
        .expect("idempotent begin");
    assert_eq!(again.transfer, first.transfer);

    let error = store
        .begin_transfer(begin(&session, &transfer, 11))
        .await
        .expect_err("different total");
    assert!(matches!(
        error,
        PortError::Conflict(ConflictKind::AlreadyExists)
    ));

    let unknown = session_id(99);
    let error = store
        .begin_transfer(begin(&unknown, &transfer_id(51), 1))
        .await
        .expect_err("unknown session");
    assert!(matches!(error, PortError::NotFound(EntityRef::Session(_))));

    let mut empty = begin(&session, &transfer_id(52), 1);
    empty.media_type.clear();
    let error = store
        .begin_transfer(empty)
        .await
        .expect_err("empty media type");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    let mut oversized = begin(&session, &transfer_id(53), 1);
    oversized.total_length = u64::MAX;
    let error = store
        .begin_transfer(oversized)
        .await
        .expect_err("over storage limit");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    store.close().await;
}

/// 摘要/长度以外的 IO 失败必须返回具名 Unavailable，而不是 panic 或静默成功。
#[tokio::test]
async fn unwritable_attachment_directory_is_an_unavailable_error() {
    let dir = temp_dir("transfer-unwritable");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    // 用同名目录占住临时文件路径：创建/打开临时文件必然失败。
    let transfer = transfer_id(60);
    let path = temporary(&dir, &transfer);
    std::fs::create_dir_all(&path).expect("directory in place of file");
    let error = store
        .begin_transfer(begin(&session, &transfer, 4))
        .await
        .expect_err("temp file cannot be created");
    assert!(matches!(
        error,
        PortError::Unavailable(UnavailableKind::IoError)
    ));
    // 失败不留下传输行。
    assert!(store.transfer(&transfer).await.expect("read").is_none());
    // 清掉占位目录，证明同一 id 之后可以正常开始（失败没有污染状态）。
    std::fs::remove_dir(&path).expect("remove directory");
    store
        .begin_transfer(begin(&session, &transfer, 4))
        .await
        .expect("begin after cleanup");
    store.close().await;
}

/// CRITICAL 回归：维护周期的孤儿回收（每 60 秒一次、以 `now` 为下界）**不得**删掉正在上传的临时文件。
///
/// 临时文件放在 `attachments/.transfers/` 子目录里，而 `sweep_orphans` 只看顶层 `is_file()` 条目；
/// 因此跨过一个维护周期后上传仍可继续。用 2100 年的 `at` 让「若被扫到就必然被删」。
#[tokio::test]
async fn sweep_orphans_keeps_active_transfers_and_upload_continues() {
    let dir = temp_dir("transfer-sweep");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    let transfer = transfer_id(70);
    let total = 64_u64;
    store
        .begin_transfer(begin(&session, &transfer, total))
        .await
        .expect("begin");
    store
        .append_chunk(chunk(&transfer, 0, chunk_at(0, 32)))
        .await
        .expect("append");
    assert!(temporary(&dir, &transfer).exists());

    let swept = store
        .sweep_orphans(
            Timestamp::new("2100-01-01T00:00:00.000Z").expect("at"),
            1000,
        )
        .await
        .expect("sweep");
    assert_eq!(swept, 0, "进行中的传输文件不在孤儿回收范围内");
    assert!(
        temporary(&dir, &transfer).exists(),
        "活动传输的临时文件不得被删除"
    );

    // 续传并封存：跨过维护周期不应破坏上传。
    store
        .append_chunk(chunk(&transfer, 32, chunk_at(32, 32)))
        .await
        .expect("append after sweep");
    let sealed = store
        .seal_transfer(seal(&transfer, total, streamed_digest(total), 2))
        .await
        .expect("seal after sweep");
    assert_eq!(sealed.length, total);
    assert!(!temporary(&dir, &transfer).exists());
    store.close().await;
}
