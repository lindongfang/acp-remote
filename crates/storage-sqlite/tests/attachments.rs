//! §5.3/§7.3：附件的内容寻址写入、`link` 与 LRU 清理。

mod support;

use acp_core::model::{AttachmentGeneration, Timestamp};
use acp_core::ports::{AttachmentStore, SessionStore, StateChange};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

fn at(hour: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{hour:02}:00:00.000Z")).expect("timestamp")
}

async fn store(dir: &std::path::Path, budget: u64) -> SqliteStore {
    let mut config = StorageConfig::new(dir);
    config.attachment_max_total_bytes = budget;
    SqliteStore::open(config, &at(0)).await.expect("open")
}

async fn session(store: &SqliteStore) -> acp_core::model::SessionId {
    store
        .commit(acp_core::ports::OwnedCommit {
            session: None,
            at: at(0),
            expected_version: None,
            state: Some(StateChange::Create(acp_core::ports::NewSession {
                title: Some("attachments".to_owned()),
                agent: acp_core::model::AgentRef::try_new(
                    acp_core::model::AgentId::new("probe-agent").expect("agent"),
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
            origin_epoch: Some(
                acp_core::model::OriginEpoch::new("6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b")
                    .expect("epoch"),
            ),
            input: None,
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id")
}

/// 内容寻址：同样的字节只落一份文件与一行元数据，`get` 原样返回。
#[tokio::test]
async fn attachments_are_content_addressed() {
    let dir = temp_dir("attachments-addressed");
    let store = store(&dir, 1024 * 1024).await;
    let bytes = b"attachment-bytes".to_vec();

    let first = store.put(&bytes, "text/plain", at(1)).await.expect("put");
    assert_eq!(first.byte_length, bytes.len() as u64);
    assert_eq!(first.media_type, "text/plain");
    assert!(first.relative_path.starts_with("attachments/"));
    assert_eq!(
        std::fs::read(dir.join(&first.relative_path)).expect("file"),
        bytes
    );

    let again = store.put(&bytes, "text/plain", at(2)).await.expect("put");
    assert_eq!(again.id, first.id, "same content, same id");
    assert_eq!(again.sha256, first.sha256);

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        1
    );
    pool.close().await;

    // 未 link 的内容也能按 id 取回（id 由内容地址确定性导出）。
    let fetched = store.get(&first.id).await.expect("get").expect("bytes");
    assert_eq!(fetched, bytes);
    assert!(
        store
            .get(
                &acp_core::model::AttachmentId::new("99999999-9999-4999-8999-999999999999")
                    .expect("id")
            )
            .await
            .expect("get")
            .is_none()
    );
    store.close().await;
}

/// 超出单文件上限 → 显式拒绝。
#[tokio::test]
async fn oversized_attachment_is_rejected() {
    let dir = temp_dir("attachments-limit");
    let mut config = StorageConfig::new(&dir);
    config.attachment_max_file_bytes = 8;
    let store = SqliteStore::open(config, &at(0)).await.expect("open");
    let error = store
        .put(b"0123456789", "text/plain", at(1))
        .await
        .expect_err("too large");
    assert!(matches!(
        error,
        acp_core::model::PortError::InvalidRequest(_)
    ));
    assert!(
        !dir.join("attachments")
            .read_dir()
            .expect("dir")
            .any(|entry| {
                entry
                    .map(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
                    .unwrap_or(false)
            })
    );
    store.close().await;
}

/// `link` 记录会话归属与 generation，并对未知会话/附件给出具名错误。
#[tokio::test]
async fn link_records_session_ownership() {
    let dir = temp_dir("attachments-link");
    let store = store(&dir, 1024 * 1024).await;
    let session = session(&store).await;
    let reference = store
        .put(b"linked", "text/plain", at(1))
        .await
        .expect("put");

    store
        .link(&session, &reference.id, AttachmentGeneration::new(3), at(2))
        .await
        .expect("link");
    store
        .link(&session, &reference.id, AttachmentGeneration::new(4), at(3))
        .await
        .expect("re-link bumps generation");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT generation FROM owned_attachment_link").await,
        4
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment_link").await,
        1
    );
    pool.close().await;

    let unknown_session = acp_core::model::SessionId::new("99999999-9999-4999-8999-999999999999")
        .expect("session id");
    let missing_session = store
        .link(
            &unknown_session,
            &reference.id,
            AttachmentGeneration::new(1),
            at(2),
        )
        .await
        .expect_err("unknown session");
    assert!(matches!(
        missing_session,
        acp_core::model::PortError::NotFound(_)
    ));

    let unknown_attachment =
        acp_core::model::AttachmentId::new("99999999-9999-4999-8999-999999999999").expect("id");
    let missing_attachment = store
        .link(
            &session,
            &unknown_attachment,
            AttachmentGeneration::new(1),
            at(2),
        )
        .await
        .expect_err("unknown attachment");
    assert!(matches!(
        missing_attachment,
        acp_core::model::PortError::InvalidRequest(_)
    ));
    store.close().await;
}

/// `prune_lru` 按 `last_used_at` 从最旧删到预算以内，行与文件一起删除。
#[tokio::test]
async fn prune_lru_removes_the_oldest_attachments_first() {
    let dir = temp_dir("attachments-lru");
    let store = store(&dir, 1024 * 1024).await;
    let mut ids = Vec::new();
    for (index, hour) in [1_u32, 2, 3].into_iter().enumerate() {
        let body = vec![u8::try_from(index).expect("byte"); 64];
        let reference = store
            .put(&body, "application/octet-stream", at(hour))
            .await
            .expect("put");
        ids.push(reference);
    }

    // 预算只够两条。
    let report = store.prune_lru(128, at(4)).await.expect("prune");
    assert_eq!(report.removed_attachments, 1);
    assert_eq!(report.freed_bytes, 64);
    assert!(!report.still_over_limit);
    assert!(
        store.get(&ids[0].id).await.expect("get").is_none(),
        "the least recently used attachment goes first"
    );
    assert!(store.get(&ids[1].id).await.expect("get").is_some());
    assert!(store.get(&ids[2].id).await.expect("get").is_some());
    assert!(!dir.join(&ids[0].relative_path).exists(), "file removed");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        2
    );
    pool.close().await;

    // 预算为 0 → 全部清空，并报告不再超限。
    let report = store.prune_lru(0, at(5)).await.expect("prune");
    assert_eq!(report.removed_attachments, 2);
    assert!(!report.still_over_limit);
    store.close().await;
}

/// §6 第 18 条：孤儿回收只删「不在表里**且** `mtime < at`」的文件；表内文件与「表外但 mtime ≥ at」的
/// 文件必须保留；`limit` 生效，`limit = 0` 不删。
///
/// 测试不改文件 mtime（std 无安全 API）：用**未来**的 `at` 让孤儿成为「陈旧」，用**过去**的 `at`
/// （2000-01-01）让刚写下的文件成为「可能正在写」。
#[tokio::test]
async fn sweep_orphans_only_removes_stale_untracked_files() {
    let dir = temp_dir("attachments-orphans");
    let store = store(&dir, 1024 * 1024).await;
    let tracked = store
        .put(b"tracked", "text/plain", at(1))
        .await
        .expect("put");
    let attachments = dir.join("attachments");

    let orphan_a = attachments.join("orphan-a");
    let orphan_b = attachments.join("orphan-b");
    std::fs::write(&orphan_a, b"stale").expect("orphan a");
    std::fs::write(&orphan_b, b"stale").expect("orphan b");

    // (1) `limit = 0` 不删任何文件。
    assert_eq!(
        store
            .sweep_orphans(Timestamp::new("2100-01-01T00:00:00.000Z").expect("at"), 0)
            .await
            .expect("sweep"),
        0
    );
    assert!(orphan_a.exists() && orphan_b.exists());

    // (2) 未来的 `at` → 两个孤儿都陈旧、被删；表内文件与表行都不受影响。
    assert_eq!(
        store
            .sweep_orphans(Timestamp::new("2100-01-01T00:00:00.000Z").expect("at"), 10)
            .await
            .expect("sweep"),
        2
    );
    assert!(!orphan_a.exists() && !orphan_b.exists(), "orphans removed");
    assert!(
        attachments
            .join(tracked.relative_path.rsplit('/').next().expect("name"))
            .exists(),
        "a tracked file must never be swept"
    );
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        1,
        "sweeping files must not touch the table"
    );
    pool.close().await;

    // (3) 刚写下的孤儿 + 过去的 `at` → mtime >= at，必须保留（可能正在写、行还没提交）。
    std::fs::write(&orphan_a, b"fresh").expect("fresh orphan");
    assert_eq!(
        store
            .sweep_orphans(Timestamp::new("2000-01-01T00:00:00.000Z").expect("at"), 10)
            .await
            .expect("sweep"),
        0
    );
    assert!(
        orphan_a.exists(),
        "a fresh orphan may be mid-write: keep it"
    );

    // (4) `limit` 生效：同一次只删 1 个。
    assert_eq!(
        store
            .sweep_orphans(Timestamp::new("2100-01-01T00:00:00.000Z").expect("at"), 1)
            .await
            .expect("sweep"),
        1
    );
    assert!(!orphan_a.exists(), "the oldest-name orphan goes first");
    store.close().await;
}

// ---------------------------------------------------------------------------------------------
// `conversation-reliability` §2.1：带附件的普通输入与受理写集同事务。
// ---------------------------------------------------------------------------------------------

use acp_core::model::{
    Actor, AttachmentId, AttachmentSubject, CommandKind, DeviceId, Digest, EventKind, EventOrigin,
    EventPayload, EventType, MessageId, PendingEvent, PromptContentBlock, PromptInput, RequestId,
    StoredPolicy, SubmissionId, TurnId, ViewJson,
};
use acp_core::ports::{
    AttachmentTransferStore, IdempotencyRecord, NewTurn, OwnedCommit, SubmissionBegin,
    SubmissionBinding, SubmissionPage, SubmissionSeal, TransferBegin, TransferChunk, TransferSeal,
    TurnChange,
};

fn user_device() -> DeviceId {
    DeviceId::new("55555555-5555-4555-8555-000000000088").expect("device")
}

fn input_actor() -> Actor {
    Actor::Device {
        device: user_device(),
    }
}

fn input_subject() -> AttachmentSubject {
    AttachmentSubject::Device(user_device())
}

fn digest_of(data: &[u8]) -> Digest {
    Digest::new(&digest_text(&String::from_utf8_lossy(data))).expect("digest")
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

async fn seal_attachment(
    store: &SqliteStore,
    session: &acp_core::model::SessionId,
) -> AttachmentId {
    let data = b"report";
    let transfer = AttachmentId::new("70000000-0000-4000-8000-000000000088").expect("id");
    store
        .begin_transfer(TransferBegin {
            transfer: transfer.clone(),
            session: session.clone(),
            subject: input_subject(),
            total_length: data.len() as u64,
            media_type: "text/plain".to_owned(),
            file_name: "report.txt".to_owned(),
            at: at(1),
        })
        .await
        .expect("begin transfer");
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
            transfer,
            length: data.len() as u64,
            sha256: digest_of(data),
            at: at(1),
        })
        .await
        .expect("seal")
        .attachment
}

async fn seal_submission(
    store: &SqliteStore,
    session: &acp_core::model::SessionId,
    submission: &SubmissionId,
    attachments: &[AttachmentId],
) {
    store
        .begin_submission(SubmissionBegin {
            submission: submission.clone(),
            session: session.clone(),
            subject: input_subject(),
            attachment_count: attachments.len() as u64,
            at: at(1),
        })
        .await
        .expect("begin submission");
    store
        .append_submission_page(SubmissionPage {
            submission: submission.clone(),
            start_position: 0,
            attachments: attachments.to_vec(),
            at: at(1),
        })
        .await
        .expect("page");
    store
        .finalize_submission(SubmissionSeal {
            submission: submission.clone(),
            session: session.clone(),
            subject: input_subject(),
            attachments: attachments.to_vec(),
            digest: manifest_digest(attachments),
            at: at(1),
        })
        .await
        .expect("finalize");
}

fn prompt_input(
    session: &acp_core::model::SessionId,
    message: &MessageId,
    turn: &TurnId,
    request: &RequestId,
    attachment: &AttachmentId,
    declared_media_type: &str,
) -> (OwnedCommit, SubmissionBinding) {
    let content = PromptInput::new(vec![
        PromptContentBlock::from_json_text(r#"{"type":"text","text":"请看附件"}"#)
            .expect("text block"),
        PromptContentBlock::from_json_text(
            &serde_json::json!({
                "type": "attachment_ref",
                "attachmentId": attachment.as_str(),
                "mediaType": declared_media_type,
                "byteLength": "6",
            })
            .to_string(),
        )
        .expect("attachment block"),
    ])
    .expect("input");
    let binding = SubmissionBinding {
        submission: SubmissionId::new("80000000-0000-4000-8000-000000000088").expect("submission"),
        attachments: vec![attachment.clone()],
        subject: input_subject(),
    };
    (
        OwnedCommit {
            session: Some(session.clone()),
            at: at(2),
            expected_version: None,
            state: None,
            turns: vec![TurnChange::Create(NewTurn {
                turn: turn.clone(),
                state: acp_core::model::TurnState::Queued,
                causation: Some(request.clone()),
                started_at: None,
            })],
            events: vec![PendingEvent::new(
                EventKind::Delta,
                EventType::new("user.message.delta").expect("event type"),
                StoredPolicy::ShortTerm,
                EventPayload::new(
                    ViewJson::new(r#"{"deltaIndex":"0","text":"请看附件"}"#).expect("view"),
                    None,
                ),
                EventOrigin::Device,
                Some(turn.clone()),
                Some(request.clone()),
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: input_actor(),
                request: request.clone(),
                command: "session.prompt".to_owned(),
                kind: CommandKind::Mutation,
                session: Some(session.clone()),
                expected_version: None,
                request_fingerprint: Digest::new(&digest_text("attachment-prompt"))
                    .expect("digest"),
                accepted_at: at(2),
            }),
            command_terminal: None,
            origin_epoch: None,
            input: Some(acp_core::model::UserInputWrite {
                message: message.clone(),
                session: session.clone(),
                turn: turn.clone(),
                request: request.clone(),
                content,
                at: at(2),
            }),
        },
        binding,
    )
}

/// 带附件的普通输入：媒体引用与消息/受理关联在同一事务里写；清单无效时零写入。
#[tokio::test]
async fn prompt_with_attachment_writes_owned_media_refs_in_the_same_transaction() {
    let dir = temp_dir("attachments-prompt-input");
    let store = store(&dir, 1024 * 1024).await;
    let session = session(&store).await;
    let attachment = seal_attachment(&store, &session).await;
    let submission = SubmissionId::new("80000000-0000-4000-8000-000000000088").expect("submission");
    seal_submission(
        &store,
        &session,
        &submission,
        std::slice::from_ref(&attachment),
    )
    .await;

    let message = MessageId::new("40000000-0000-4000-8000-000000000088").expect("message");
    let turn = TurnId::new("33333333-3333-4333-8333-000000000088").expect("turn");
    let request = RequestId::new("66666666-6666-4666-8666-000000000088").expect("request");
    let (commit, binding) = prompt_input(
        &session,
        &message,
        &turn,
        &request,
        &attachment,
        "text/plain",
    );
    store
        .commit_with_submission(commit, Some(binding.clone()))
        .await
        .expect("accepted prompt");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let (kind, position, media_attachment): (String, i64, String) = sqlx::query_as(
        "SELECT kind, position, attachment_id FROM owned_media_ref WHERE message_id = ?1",
    )
    .bind(message.as_str())
    .fetch_one(&pool)
    .await
    .expect("media ref");
    assert_eq!(kind, "file", "kind 取自附件权威行的 media_type");
    assert_eq!(position, 0);
    assert_eq!(media_attachment, attachment.as_str());
    let attachment_count: i64 =
        sqlx::query_scalar("SELECT attachment_count FROM owned_user_input WHERE message_id = ?1")
            .bind(message.as_str())
            .fetch_one(&pool)
            .await
            .expect("user input");
    assert_eq!(attachment_count, 1);
    pool.close().await;

    // 清单不可用（未封存/未知 submission）：整事务失败，不得留下消息、受理关联或媒体引用。
    let second_message = MessageId::new("40000000-0000-4000-8000-000000000089").expect("message");
    let second_turn = TurnId::new("33333333-3333-4333-8333-000000000089").expect("turn");
    let second_request = RequestId::new("66666666-6666-4666-8666-000000000089").expect("request");
    let (commit, _) = prompt_input(
        &session,
        &second_message,
        &second_turn,
        &second_request,
        &attachment,
        "text/plain",
    );
    let unknown = SubmissionBinding {
        submission: SubmissionId::new("80000000-0000-4000-8000-000000000099").expect("submission"),
        attachments: vec![attachment.clone()],
        subject: input_subject(),
    };
    let error = store
        .commit_with_submission(commit, Some(unknown))
        .await
        .expect_err("unknown submission");
    assert!(matches!(
        error,
        acp_core::model::PortError::NotFound(_) | acp_core::model::PortError::InvalidRequest(_)
    ));
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_message").await,
        1,
        "失败提交不得留下第二条消息"
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_media_ref").await,
        1
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_user_input").await,
        1
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_turn").await,
        1,
        "失败提交的 turn 也必须回滚"
    );
    pool.close().await;
}
