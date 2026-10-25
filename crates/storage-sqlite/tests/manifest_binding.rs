//! §6 第 22 条/§7.6：附件清单分页、封存与 prompt 受理的同事务绑定。
//!
//! 每个用例都断言「失败时命令不被受理、事件不落库、附件不被错误派发」——这是绑定必须与受理
//! 同事务的原因，而不是事后再补一条关联。

mod support;

use acp_core::model::{
    Actor, AgentId, AgentRef, AttachmentId, AttachmentSubject, CommandKind, DeviceId, Digest,
    EntityRef, EventKind, EventOrigin, EventPayload, EventType, OriginEpoch, PendingEvent,
    PortError, RequestId, SessionId, StoredPolicy, SubmissionId, Timestamp, ViewJson,
};
use acp_core::ports::{
    AttachmentRead, AttachmentTransferStore, IdempotencyRecord, NewSession, OwnedCommit,
    SessionStore, StateChange, SubmissionBegin, SubmissionBinding, SubmissionPage, SubmissionSeal,
    TransferBegin, TransferChunk, TransferSeal,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const ORIGIN_EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
const DEVICE: &str = "55555555-5555-4555-8555-555555555555";

fn at(hour: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{hour:02}:00:00.000Z")).expect("timestamp")
}

fn uuid_text(n: u64) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

fn transfer_id(n: u64) -> AttachmentId {
    AttachmentId::new(&uuid_text(n)).expect("attachment id")
}

fn submission_id(n: u64) -> SubmissionId {
    SubmissionId::new(&uuid_text(n)).expect("submission id")
}

fn request(n: u64) -> RequestId {
    RequestId::new(&uuid_text(n)).expect("request id")
}

fn subject() -> AttachmentSubject {
    AttachmentSubject::Device(DeviceId::new(DEVICE).expect("device"))
}

fn actor() -> Actor {
    Actor::Device {
        device: DeviceId::new(DEVICE).expect("device"),
    }
}

async fn store(dir: &std::path::Path) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at(0))
        .await
        .expect("open store")
}

async fn create_session(store: &SqliteStore, title: &str) -> SessionId {
    store
        .commit(OwnedCommit {
            session: None,
            at: at(0),
            expected_version: None,
            state: Some(StateChange::Create(NewSession {
                title: Some(title.to_owned()),
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

fn digest_of(data: &[u8]) -> Digest {
    Digest::new(&digest_text(&String::from_utf8_lossy(data))).expect("digest")
}

async fn seal_bytes(
    store: &SqliteStore,
    session: &SessionId,
    transfer: &AttachmentId,
    data: &[u8],
) -> AttachmentId {
    store
        .begin_transfer(TransferBegin {
            transfer: transfer.clone(),
            session: session.clone(),
            subject: subject(),
            total_length: data.len() as u64,
            media_type: "application/octet-stream".to_owned(),
            file_name: "payload.bin".to_owned(),
            at: at(1),
        })
        .await
        .expect("begin");
    store
        .append_chunk(TransferChunk {
            transfer: transfer.clone(),
            offset: 0,
            data: data.to_vec(),
            at: at(1),
        })
        .await
        .expect("append");
    store
        .seal_transfer(TransferSeal {
            transfer: transfer.clone(),
            length: data.len() as u64,
            sha256: digest_of(data),
            at: at(1),
        })
        .await
        .expect("seal")
        .attachment
}

fn manifest_digest(attachments: &[AttachmentId]) -> Digest {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    for attachment in attachments {
        let bytes = attachment.as_str().as_bytes();
        hasher.update((bytes.len() as u32).to_be_bytes());
        hasher.update(bytes);
    }
    Digest::new(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize()))
        .expect("digest")
}

fn begin_submission(session: &SessionId, submission: &SubmissionId, count: u64) -> SubmissionBegin {
    SubmissionBegin {
        submission: submission.clone(),
        session: session.clone(),
        subject: subject(),
        attachment_count: count,
        at: at(1),
    }
}

fn page(submission: &SubmissionId, start: u64, ids: &[AttachmentId]) -> SubmissionPage {
    SubmissionPage {
        submission: submission.clone(),
        start_position: start,
        attachments: ids.to_vec(),
        at: at(1),
    }
}

fn seal_submission(
    submission: &SubmissionId,
    session: &SessionId,
    ids: &[AttachmentId],
    digest: Digest,
) -> SubmissionSeal {
    SubmissionSeal {
        submission: submission.clone(),
        session: session.clone(),
        subject: subject(),
        attachments: ids.to_vec(),
        digest,
        at: at(1),
    }
}

fn prompt_event(session: &SessionId, message: &str) -> PendingEvent {
    PendingEvent::new(
        EventKind::Structured,
        EventType::new("user.message.completed").expect("event type"),
        StoredPolicy::Durable,
        EventPayload::new(
            ViewJson::new(&format!(
                r#"{{"messageId":"{}","sessionId":"{}","text":"{message}"}}"#,
                uuid_text(900),
                session.as_str()
            ))
            .expect("view"),
            None,
        ),
        EventOrigin::Device,
        None,
        None,
    )
}

/// 一次 prompt 受理提交：幂等行 + 事件 +（可选）清单绑定，全部同事务。
fn acceptance(
    session: &SessionId,
    request_number: u64,
    binding: Option<SubmissionBinding>,
    message: &str,
) -> (OwnedCommit, Option<SubmissionBinding>) {
    (
        OwnedCommit {
            session: Some(session.clone()),
            at: at(2),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![prompt_event(session, message)],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: actor(),
                request: request(request_number),
                command: "session.prompt".to_owned(),
                kind: CommandKind::Mutation,
                session: Some(session.clone()),
                expected_version: None,
                request_fingerprint: Digest::new(&digest_text(message)).expect("digest"),
                accepted_at: at(2),
            }),
            command_terminal: None,
            origin_epoch: None,
            input: None,
        },
        binding,
    )
}

async fn counts(dir: &std::path::Path) -> (i64, i64, i64) {
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let commands = scalar_i64(&pool, "SELECT COUNT(*) FROM owned_command").await;
    let events = scalar_i64(&pool, "SELECT COUNT(*) FROM owned_event").await;
    let links = scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment_link").await;
    pool.close().await;
    (commands, events, links)
}

/// 清单分页幂等、整批校验与封存后读取。
#[tokio::test]
async fn manifest_pages_are_idempotent_and_finalize_rechecks_everything() {
    let dir = temp_dir("manifest-pages");
    let store = store(&dir).await;
    let session = create_session(&store, "manifest").await;
    let a = seal_bytes(&store, &session, &transfer_id(1), b"alpha").await;
    let b = seal_bytes(&store, &session, &transfer_id(2), b"bravo").await;
    let c = seal_bytes(&store, &session, &transfer_id(3), b"charlie").await;
    let submission = submission_id(10);

    let progress = store
        .begin_submission(begin_submission(&session, &submission, 3))
        .await
        .expect("begin submission");
    assert_eq!((progress.registered, progress.attachment_count), (0, 3));
    assert!(!progress.sealed);
    // 未封存清单不可被 prompt 引用。
    assert!(store.submission(&submission).await.expect("read").is_none());

    let first = store
        .append_submission_page(page(&submission, 0, &[a.clone(), b.clone()]))
        .await
        .expect("page 1");
    assert_eq!(first.registered, 2);
    // 同一页重复提交是幂等的。
    let repeated = store
        .append_submission_page(page(&submission, 0, &[a.clone(), b.clone()]))
        .await
        .expect("page 1 again");
    assert_eq!(repeated.registered, 2);
    // 同位置不同内容 → 冲突。
    let error = store
        .append_submission_page(page(&submission, 0, std::slice::from_ref(&c)))
        .await
        .expect_err("conflicting page");
    assert!(matches!(
        error,
        PortError::Conflict(acp_core::model::ConflictKind::AlreadyExists)
    ));
    // 越过声明数量 → 拒绝。
    let error = store
        .append_submission_page(page(&submission, 3, std::slice::from_ref(&c)))
        .await
        .expect_err("past declared count");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    let second = store
        .append_submission_page(page(&submission, 2, std::slice::from_ref(&c)))
        .await
        .expect("page 2");
    assert_eq!(second.registered, 3);

    // 摘要 / 顺序不符 → 整批拒绝，清单保持 open。
    let error = store
        .finalize_submission(seal_submission(
            &submission,
            &session,
            &[a.clone(), b.clone(), c.clone()],
            Digest::new(&digest_text("wrong")).expect("digest"),
        ))
        .await
        .expect_err("digest mismatch");
    assert!(matches!(error, PortError::InvalidRequest(_)));
    let error = store
        .finalize_submission(seal_submission(
            &submission,
            &session,
            &[c.clone(), b.clone(), a.clone()],
            manifest_digest(&[c.clone(), b.clone(), a.clone()]),
        ))
        .await
        .expect_err("page order mismatch");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    let ids = vec![a.clone(), b.clone(), c.clone()];
    let sealed = store
        .finalize_submission(seal_submission(
            &submission,
            &session,
            &ids,
            manifest_digest(&ids),
        ))
        .await
        .expect("finalize");
    assert_eq!(sealed.attachments, ids);
    assert_eq!(
        store.submission(&submission).await.expect("read"),
        Some(sealed.clone())
    );
    // 重复封存同值幂等；不同值冲突。
    assert_eq!(
        store
            .finalize_submission(seal_submission(
                &submission,
                &session,
                &ids,
                manifest_digest(&ids),
            ))
            .await
            .expect("idempotent finalize"),
        sealed
    );
    let error = store
        .finalize_submission(seal_submission(
            &submission,
            &session,
            std::slice::from_ref(&a),
            manifest_digest(std::slice::from_ref(&a)),
        ))
        .await
        .expect_err("different sealed list");
    assert!(matches!(
        error,
        PortError::Conflict(acp_core::model::ConflictKind::AlreadyExists)
    ));

    // 绑定前附件不属于会话 → 读取 NotFound；绑定后可读。
    let error = store
        .read_chunk(&AttachmentRead {
            attachment: a.clone(),
            session: session.clone(),
            offset: 0,
            max_length: 16,
        })
        .await
        .expect_err("unbound attachment");
    assert!(matches!(error, PortError::NotFound(_)));

    let (commit, binding) = acceptance(
        &session,
        100,
        Some(SubmissionBinding {
            submission: submission.clone(),
            attachments: ids.clone(),
            subject: subject(),
        }),
        "with attachments",
    );
    store
        .commit_with_submission(commit, binding)
        .await
        .expect("bind on acceptance");
    assert_eq!(counts(&dir).await, (1, 1, 3));
    let read = store
        .read_chunk(&AttachmentRead {
            attachment: a,
            session: session.clone(),
            offset: 0,
            max_length: 16,
        })
        .await
        .expect("read bound attachment");
    assert_eq!(read.data, b"alpha");

    store.close().await;
}

/// 重试同一 `requestId` 不重复绑定、不重复写事件；绑定失败时幂等行/事件/链接一条都不落。
#[tokio::test]
async fn submission_binding_is_atomic_with_the_acceptance() {
    let dir = temp_dir("manifest-atomic");
    let store = store(&dir).await;
    let session = create_session(&store, "atomic").await;
    let other = create_session(&store, "other").await;
    let a = seal_bytes(&store, &session, &transfer_id(1), b"alpha").await;
    let b = seal_bytes(&store, &session, &transfer_id(2), b"bravo").await;
    let submission = submission_id(20);
    store
        .begin_submission(begin_submission(&session, &submission, 2))
        .await
        .expect("begin");
    store
        .append_submission_page(page(&submission, 0, &[a.clone(), b.clone()]))
        .await
        .expect("page");
    let ids = vec![a.clone(), b.clone()];
    store
        .finalize_submission(seal_submission(
            &submission,
            &session,
            &ids,
            manifest_digest(&ids),
        ))
        .await
        .expect("finalize");

    // 跨会话引用：整次受理回滚。
    let (commit, binding) = acceptance(
        &other,
        200,
        Some(SubmissionBinding {
            submission: submission.clone(),
            attachments: ids.clone(),
            subject: subject(),
        }),
        "wrong session",
    );
    let error = store
        .commit_with_submission(commit, binding)
        .await
        .expect_err("cross-session submission");
    assert!(matches!(error, PortError::InvalidRequest(_)));
    assert_eq!(
        counts(&dir).await,
        (0, 0, 0),
        "失败不得留下幂等行/事件/链接"
    );

    // 清单条目与登记不一致：同样回滚。
    let (commit, binding) = acceptance(
        &session,
        201,
        Some(SubmissionBinding {
            submission: submission.clone(),
            attachments: vec![a.clone()],
            subject: subject(),
        }),
        "partial binding",
    );
    let error = store
        .commit_with_submission(commit, binding)
        .await
        .expect_err("binding length mismatch");
    assert!(matches!(error, PortError::InvalidRequest(_)));
    assert_eq!(counts(&dir).await, (0, 0, 0));

    // 未知清单：NotFound，且零写入。
    let (commit, binding) = acceptance(
        &session,
        202,
        Some(SubmissionBinding {
            submission: submission_id(99),
            attachments: ids.clone(),
            subject: subject(),
        }),
        "unknown submission",
    );
    let error = store
        .commit_with_submission(commit, binding)
        .await
        .expect_err("unknown submission");
    assert!(matches!(
        error,
        PortError::NotFound(EntityRef::Submission(_))
    ));
    assert_eq!(counts(&dir).await, (0, 0, 0));

    // 成功受理。
    let (commit, binding) = acceptance(
        &session,
        203,
        Some(SubmissionBinding {
            submission: submission.clone(),
            attachments: ids.clone(),
            subject: subject(),
        }),
        "accepted",
    );
    let first = store
        .commit_with_submission(commit, binding)
        .await
        .expect("accept");
    assert!(first.replayed.is_none());
    assert_eq!(counts(&dir).await, (1, 1, 2));

    // 同 requestId 重试：返回首次结果，不重复绑定、不追加事件。
    let (commit, binding) = acceptance(
        &session,
        203,
        Some(SubmissionBinding {
            submission: submission.clone(),
            attachments: ids.clone(),
            subject: subject(),
        }),
        "accepted",
    );
    let replay = store
        .commit_with_submission(commit, binding)
        .await
        .expect("replay");
    assert!(replay.replayed.is_some());
    assert_eq!(counts(&dir).await, (1, 1, 2));

    store.close().await;
}

/// 清单主体在**受理事务内**复核：直接调用 `commit_with_submission` 也绕不过去。
#[tokio::test]
async fn binding_rechecks_the_authenticated_subject() {
    let dir = temp_dir("manifest-subject");
    let store = store(&dir).await;
    let session = create_session(&store, "subject").await;
    let a = seal_bytes(&store, &session, &transfer_id(70), b"alpha").await;
    let submission = submission_id(70);
    store
        .begin_submission(begin_submission(&session, &submission, 1))
        .await
        .expect("begin");
    store
        .append_submission_page(page(&submission, 0, std::slice::from_ref(&a)))
        .await
        .expect("page");
    store
        .finalize_submission(seal_submission(
            &submission,
            &session,
            std::slice::from_ref(&a),
            manifest_digest(std::slice::from_ref(&a)),
        ))
        .await
        .expect("finalize");

    let other_subject = AttachmentSubject::Node {
        access_node: acp_core::model::NodeId::new(&uuid_text(900)).expect("node id"),
    };
    let (commit, binding) = acceptance(
        &session,
        400,
        Some(SubmissionBinding {
            submission: submission.clone(),
            attachments: vec![a.clone()],
            subject: other_subject,
        }),
        "wrong subject",
    );
    let error = store
        .commit_with_submission(commit, binding)
        .await
        .expect_err("subject mismatch");
    assert!(matches!(error, PortError::InvalidRequest(_)), "{error:?}");
    assert_eq!(counts(&dir).await, (0, 0, 0), "主体不符不得受理或绑定");

    // 用正确主体重试同一 requestId：清单仍在（失败回滚），可以受理。
    let (commit, binding) = acceptance(
        &session,
        400,
        Some(SubmissionBinding {
            submission,
            attachments: vec![a],
            subject: subject(),
        }),
        "right subject",
    );
    store
        .commit_with_submission(commit, binding)
        .await
        .expect("accept with the authenticated subject");
    assert_eq!(counts(&dir).await, (1, 1, 1));

    store.close().await;
}

/// 部分失败与磁盘失败：未封存清单、被取消的传输、缺失的附件文件都不得被受理。
#[tokio::test]
async fn partial_manifests_and_missing_bytes_never_dispatch() {
    let dir = temp_dir("manifest-failures");
    let store = store(&dir).await;
    let session = create_session(&store, "failures").await;
    let a = seal_bytes(&store, &session, &transfer_id(1), b"alpha").await;
    let b = seal_bytes(&store, &session, &transfer_id(2), b"bravo").await;

    // 只登记了一页的清单不能封存。
    let partial = submission_id(30);
    store
        .begin_submission(begin_submission(&session, &partial, 2))
        .await
        .expect("begin partial");
    store
        .append_submission_page(page(&partial, 0, std::slice::from_ref(&a)))
        .await
        .expect("page");
    let error = store
        .finalize_submission(seal_submission(
            &partial,
            &session,
            &[a.clone(), b.clone()],
            manifest_digest(&[a.clone(), b.clone()]),
        ))
        .await
        .expect_err("incomplete manifest");
    assert!(matches!(error, PortError::InvalidRequest(_)));
    let (commit, binding) = acceptance(
        &session,
        300,
        Some(SubmissionBinding {
            submission: partial.clone(),
            attachments: vec![a.clone(), b.clone()],
            subject: subject(),
        }),
        "partial",
    );
    let error = store
        .commit_with_submission(commit, binding)
        .await
        .expect_err("unsealed manifest");
    assert!(matches!(error, PortError::InvalidRequest(_)));
    assert_eq!(counts(&dir).await, (0, 0, 0));

    // 被取消的传输不能进入清单。
    let cancelled = transfer_id(40);
    store
        .begin_transfer(TransferBegin {
            transfer: cancelled.clone(),
            session: session.clone(),
            subject: subject(),
            total_length: 1,
            media_type: "application/octet-stream".to_owned(),
            file_name: "cancelled".to_owned(),
            at: at(1),
        })
        .await
        .expect("begin cancelled");
    store
        .abort_transfer(&cancelled, at(2))
        .await
        .expect("abort");
    let aborted = submission_id(31);
    store
        .begin_submission(begin_submission(&session, &aborted, 1))
        .await
        .expect("begin aborted");
    store
        .append_submission_page(page(&aborted, 0, &[cancelled]))
        .await
        .expect("page");
    let error = store
        .finalize_submission(seal_submission(
            &aborted,
            &session,
            &[transfer_id(40)],
            manifest_digest(&[transfer_id(40)]),
        ))
        .await
        .expect_err("aborted transfer is not sealed");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    // 附件字节被删除：清单封存成功，但受理时复核失败并整体回滚。
    let missing_transfer = transfer_id(41);
    let missing = seal_bytes(&store, &session, &missing_transfer, b"missing").await;
    let sealed_submission = submission_id(32);
    store
        .begin_submission(begin_submission(&session, &sealed_submission, 1))
        .await
        .expect("begin missing");
    store
        .append_submission_page(page(&sealed_submission, 0, std::slice::from_ref(&missing)))
        .await
        .expect("page");
    store
        .finalize_submission(seal_submission(
            &sealed_submission,
            &session,
            std::slice::from_ref(&missing),
            manifest_digest(std::slice::from_ref(&missing)),
        ))
        .await
        .expect("finalize missing");
    std::fs::remove_file(
        dir.join("attachments")
            .join(digest_text(&String::from_utf8_lossy(b"missing"))),
    )
    .expect("remove attachment bytes");
    let (commit, binding) = acceptance(
        &session,
        301,
        Some(SubmissionBinding {
            submission: sealed_submission,
            attachments: vec![missing],
            subject: subject(),
        }),
        "missing bytes",
    );
    let error = store
        .commit_with_submission(commit, binding)
        .await
        .expect_err("missing bytes");
    assert!(matches!(error, PortError::Corrupt(_)));
    assert_eq!(counts(&dir).await, (0, 0, 0), "磁盘失败不得受理命令");

    // 会话仍可正常受理不带附件的 prompt（失败没有污染连接/库）。
    let (commit, binding) = acceptance(&session, 302, None, "plain");
    store
        .commit_with_submission(commit, binding)
        .await
        .expect("plain prompt");
    assert_eq!(counts(&dir).await, (1, 1, 0));

    store.close().await;
}

/// 同一内容被两个独立传输封存后只存一份字节；清单引用不同 transferId 时按实际附件标识去重。
#[tokio::test]
async fn duplicate_content_in_a_manifest_is_rejected() {
    let dir = temp_dir("manifest-duplicate");
    let store = store(&dir).await;
    let session = create_session(&store, "duplicate").await;
    let first = seal_bytes(&store, &session, &transfer_id(50), b"same").await;
    let second = seal_bytes(&store, &session, &transfer_id(51), b"same").await;
    assert_eq!(first, second, "内容寻址：相同字节只产生一个附件标识");

    let submission = submission_id(60);
    store
        .begin_submission(begin_submission(&session, &submission, 2))
        .await
        .expect("begin");
    store
        .append_submission_page(page(&submission, 0, &[first.clone(), second.clone()]))
        .await
        .expect("page");
    let error = store
        .finalize_submission(seal_submission(
            &submission,
            &session,
            &[first.clone(), second.clone()],
            manifest_digest(&[first.clone(), second.clone()]),
        ))
        .await
        .expect_err("duplicate attachment in one manifest");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    store.close().await;
}
