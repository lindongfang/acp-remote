//! 附件/清单的生命周期（§6 第 18/22 条、§7.5/§7.6、`conversation-attachments` 的保留要求）。
//!
//! 五个不变量：
//!
//! - 已封存但未绑定的清单不能在 24 小时后继续把附件钉成 GC 根；
//! - 成功绑定会消费清单，附件改由会话绑定/媒体引用保护；
//! - 内容寻址去重复用的旧内容按「最近一次使用」判定保留期：刚绑定的链接不被下一轮 `prune` 回收；
//! - 消息保留期到期后，附件与其文件被回收（存储预算清理与按引用清理都覆盖）；
//! - 附件被回收后，指向它的已封存传输同步变为 `expired`，不出现「sealed 但 sealed: None」。

mod support;

use acp_core::model::{
    Actor, AgentId, AgentRef, AttachmentGeneration, AttachmentId, AttachmentSubject,
    AttachmentTransferState, CommandKind, DeviceId, Digest, EventKind, EventOrigin, EventPayload,
    EventType, OriginEpoch, PendingEvent, RequestId, SessionId, StoredPolicy, SubmissionId,
    Timestamp, ViewJson,
};
use acp_core::ports::{
    AttachmentRead, AttachmentStore, AttachmentTransferStore, IdempotencyRecord, NewSession,
    OwnedCommit, RetentionPolicy, SessionStore, StateChange, SubmissionBegin, SubmissionBinding,
    SubmissionPage, SubmissionSeal, TransferBegin, TransferChunk, TransferSeal,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
const DEVICE: &str = "55555555-5555-4555-8555-555555555555";

fn at(hours: u32) -> Timestamp {
    let day = 18 + hours / 24;
    let hour = hours % 24;
    Timestamp::new(&format!("2026-09-{day:02}T{hour:02}:00:00.000Z")).expect("timestamp")
}

fn later(day: u32) -> Timestamp {
    Timestamp::new(&format!("2027-06-{day:02}T00:00:00.000Z")).expect("timestamp")
}

fn uuid_text(n: u64) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

fn subject() -> AttachmentSubject {
    AttachmentSubject::Device(DeviceId::new(DEVICE).expect("device"))
}

fn actor() -> Actor {
    Actor::Device {
        device: DeviceId::new(DEVICE).expect("device"),
    }
}

fn digest_of(data: &[u8]) -> Digest {
    Digest::new(&digest_text(&String::from_utf8_lossy(data))).expect("digest")
}

fn policy() -> RetentionPolicy {
    RetentionPolicy {
        transcript_retention_days: 90,
        sync_event_retention_days: 7,
        audit_retention_days: 365,
        max_total_size_bytes: 64 * 1024 * 1024,
        max_session_size_bytes: 32 * 1024 * 1024,
        persist_deltas: false,
    }
}

async fn open(dir: &std::path::Path) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at(0))
        .await
        .expect("open")
}

async fn create_session(store: &SqliteStore) -> SessionId {
    store
        .commit(OwnedCommit {
            session: None,
            at: at(0),
            expected_version: None,
            state: Some(StateChange::Create(NewSession {
                title: Some("lifecycle".to_owned()),
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
            origin_epoch: Some(OriginEpoch::new(EPOCH).expect("epoch")),
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id")
}

async fn seal_bytes(
    store: &SqliteStore,
    session: &SessionId,
    transfer: &AttachmentId,
    data: &[u8],
) -> AttachmentId {
    seal_bytes_at(store, session, transfer, data, at(1)).await
}

async fn seal_bytes_at(
    store: &SqliteStore,
    session: &SessionId,
    transfer: &AttachmentId,
    data: &[u8],
    at: Timestamp,
) -> AttachmentId {
    store
        .begin_transfer(TransferBegin {
            transfer: transfer.clone(),
            session: session.clone(),
            subject: subject(),
            total_length: data.len() as u64,
            media_type: "application/octet-stream".to_owned(),
            file_name: "payload.bin".to_owned(),
            at: at.clone(),
        })
        .await
        .expect("begin");
    store
        .append_chunk(TransferChunk {
            transfer: transfer.clone(),
            offset: 0,
            data: data.to_vec(),
            at: at.clone(),
        })
        .await
        .expect("append");
    store
        .seal_transfer(TransferSeal {
            transfer: transfer.clone(),
            length: data.len() as u64,
            sha256: digest_of(data),
            at,
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

async fn seal_manifest(
    store: &SqliteStore,
    session: &SessionId,
    submission: &SubmissionId,
    attachments: &[AttachmentId],
) {
    seal_manifest_at(store, session, submission, attachments, at(1)).await
}

async fn seal_manifest_at(
    store: &SqliteStore,
    session: &SessionId,
    submission: &SubmissionId,
    attachments: &[AttachmentId],
    at: Timestamp,
) {
    store
        .begin_submission(SubmissionBegin {
            submission: submission.clone(),
            session: session.clone(),
            subject: subject(),
            attachment_count: attachments.len() as u64,
            at: at.clone(),
        })
        .await
        .expect("begin submission");
    store
        .append_submission_page(SubmissionPage {
            submission: submission.clone(),
            start_position: 0,
            attachments: attachments.to_vec(),
            at: at.clone(),
        })
        .await
        .expect("page");
    store
        .finalize_submission(SubmissionSeal {
            submission: submission.clone(),
            session: session.clone(),
            subject: subject(),
            attachments: attachments.to_vec(),
            digest: manifest_digest(attachments),
            at,
        })
        .await
        .expect("finalize");
}

fn prompt_event(session: &SessionId) -> PendingEvent {
    let view = format!(
        r#"{{"messageId":"{}","sessionId":"{}","text":"with attachments"}}"#,
        uuid_text(900),
        session.as_str()
    );
    PendingEvent::new(
        EventKind::Structured,
        EventType::new("user.message.completed").expect("event type"),
        StoredPolicy::Durable,
        EventPayload::new(ViewJson::new(&view).expect("view"), None),
        EventOrigin::Device,
        None,
        None,
    )
}

fn acceptance(session: &SessionId, request: u64) -> OwnedCommit {
    OwnedCommit {
        session: Some(session.clone()),
        at: at(2),
        expected_version: None,
        state: None,
        turns: Vec::new(),
        events: vec![prompt_event(session)],
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: Some(IdempotencyRecord {
            actor: actor(),
            request: RequestId::new(&uuid_text(request)).expect("request"),
            command: "session.prompt".to_owned(),
            kind: CommandKind::Mutation,
            session: Some(session.clone()),
            expected_version: None,
            request_fingerprint: Digest::new(&digest_text("accept")).expect("digest"),
            accepted_at: at(2),
        }),
        command_terminal: None,
        origin_epoch: None,
    }
}

async fn table_count(dir: &std::path::Path, table: &str) -> i64 {
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let count = scalar_i64(&pool, &format!("SELECT COUNT(*) FROM {table}")).await;
    pool.close().await;
    count
}

/// 已封存但未绑定的清单在 24 小时后过期：条目解开对附件的引用，预算清理随后可回收它。
#[tokio::test]
async fn sealed_manifests_expire_and_stop_pinning_attachments() {
    let dir = temp_dir("lifecycle-manifest-expiry");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    let attachment = seal_bytes(
        &store,
        &session,
        &AttachmentId::new(&uuid_text(1)).expect("attachment id"),
        b"alpha",
    )
    .await;
    seal_manifest(
        &store,
        &session,
        &SubmissionId::new(&uuid_text(10)).expect("submission id"),
        std::slice::from_ref(&attachment),
    )
    .await;

    // 未到期：清单条目把附件挡在预算清理之外。
    let report = store.prune_lru(0, at(2)).await.expect("prune early");
    assert_eq!(report.removed_attachments, 0, "活跃清单保护附件");
    assert_eq!(table_count(&dir, "owned_attachment").await, 1);

    // 超过 24 小时：清单过期（条目级联删除）。
    assert_eq!(
        store.prune_transfers(at(26), 100).await.expect("prune"),
        0,
        "没有 receiving 传输需要过期"
    );
    assert_eq!(table_count(&dir, "owned_attachment_submission").await, 0);
    assert_eq!(
        table_count(&dir, "owned_attachment_submission_item").await,
        0
    );

    // 预算清理现在可以真正回收字节。
    let report = store.prune_lru(0, at(27)).await.expect("prune budget");
    assert_eq!(report.removed_attachments, 1);
    assert!(report.freed_bytes > 0);
    assert_eq!(table_count(&dir, "owned_attachment").await, 0);
    let file = dir.join("attachments").join(digest_of(b"alpha").as_str());
    assert!(!file.exists(), "附件文件随之删除");
    store.close().await;
}

/// `SessionStore::prune` 也驱动清单过期（Daemon 的后台清理只调 prune，不调 prune_transfers）。
#[tokio::test]
async fn prune_drives_the_manifest_lifecycle() {
    let dir = temp_dir("lifecycle-prune-drives");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    let attachment = seal_bytes(
        &store,
        &session,
        &AttachmentId::new(&uuid_text(2)).expect("attachment id"),
        b"bravo",
    )
    .await;
    seal_manifest(
        &store,
        &session,
        &SubmissionId::new(&uuid_text(11)).expect("submission id"),
        &[attachment],
    )
    .await;

    let report = store.prune(policy(), at(26)).await.expect("prune");
    assert_eq!(table_count(&dir, "owned_attachment_submission").await, 0);
    // 同一轮里按引用 GC 回收（闲置 > 24 小时、无引用）。
    assert!(report.removed_attachments >= 1);
    assert_eq!(table_count(&dir, "owned_attachment").await, 0);
    store.close().await;
}

/// 成功绑定会消费清单；消息保留期到期后，会话绑定随之失效，附件被回收。
#[tokio::test]
async fn message_retention_reclaims_bound_attachments() {
    let dir = temp_dir("lifecycle-retention");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    let attachment = seal_bytes(
        &store,
        &session,
        &AttachmentId::new(&uuid_text(3)).expect("attachment id"),
        b"charlie",
    )
    .await;
    let submission = SubmissionId::new(&uuid_text(12)).expect("submission id");
    seal_manifest(
        &store,
        &session,
        &submission,
        std::slice::from_ref(&attachment),
    )
    .await;

    store
        .commit_with_submission(
            acceptance(&session, 20),
            Some(SubmissionBinding {
                submission,
                attachments: vec![attachment.clone()],
                subject: subject(),
            }),
        )
        .await
        .expect("accept with attachments");
    // 清单已被消费：条目不再是 GC 根（附件由会话绑定保护）。
    assert_eq!(table_count(&dir, "owned_attachment_submission").await, 0);
    assert_eq!(table_count(&dir, "owned_attachment_link").await, 1);
    assert_eq!(table_count(&dir, "owned_attachment").await, 1);

    // 消息保留期（90 天）到期：在 2027-06-01 的清理里，2026-09 的附件已过了正文窗口。
    store.prune(policy(), later(1)).await.expect("prune");
    assert_eq!(table_count(&dir, "owned_attachment_link").await, 0);
    assert_eq!(table_count(&dir, "owned_attachment").await, 0);
    assert!(
        !dir.join("attachments")
            .join(digest_of(b"charlie").as_str())
            .exists(),
        "消息保留期之后附件文件被回收"
    );
    store.close().await;
}

/// 消息的媒体引用（`owned_media_ref`）在消息清理时级联删除；最后有效引用消失后内容才可回收。
#[tokio::test]
async fn message_retention_reclaims_media_references() {
    let dir = temp_dir("lifecycle-media");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    let attachment = store.put(b"delta", "image/png", at(1)).await.expect("put");
    let message = uuid_text(950);
    let pool = raw_write_pool(&dir.join("acp-remote.sqlite3")).await;
    sqlx::query(
        "INSERT INTO owned_message (message_id, session_id, turn_id, channel, state, \
         content_json, partial_reason, created_at, updated_at) \
         VALUES (?1, ?2, NULL, 'reply', 'completed', '[]', NULL, ?3, ?3)",
    )
    .bind(&message)
    .bind(session.as_str())
    .bind(at(1).as_str())
    .execute(&pool)
    .await
    .expect("message row");
    sqlx::query(
        "INSERT INTO owned_media_ref (message_id, position, attachment_id, kind) \
         VALUES (?1, 0, ?2, 'image')",
    )
    .bind(&message)
    .bind(attachment.id.as_str())
    .execute(&pool)
    .await
    .expect("media ref");
    pool.close().await;

    // 保留期内：媒体引用保护附件。
    store.prune(policy(), at(2)).await.expect("prune early");
    assert_eq!(table_count(&dir, "owned_attachment").await, 1);

    store.prune(policy(), later(1)).await.expect("prune");
    assert_eq!(table_count(&dir, "owned_message").await, 0);
    assert_eq!(table_count(&dir, "owned_media_ref").await, 0);
    assert_eq!(table_count(&dir, "owned_attachment").await, 0);
    assert!(!dir.join(&attachment.relative_path).exists());
    store.close().await;
}

/// 24 小时闲置窗口的按引用 GC 不得误删仍被引用的内容：会话绑定（`owned_attachment_link`）与
/// 消息媒体引用（`owned_media_ref`）都是 GC 根，即使附件闲置超过 24 小时也应保留；只有既无绑定、
/// 又无媒体引用、也不在提交清单里的孤儿才被回收（同一轮里作为对照）。
#[tokio::test]
async fn idle_gc_keeps_attachments_that_are_still_referenced() {
    let dir = temp_dir("lifecycle-idle-gc-references");
    let store = open(&dir).await;
    let session = create_session(&store).await;

    // A：经提交绑定保护的附件（owned_attachment_link）。
    let bound = seal_bytes(
        &store,
        &session,
        &AttachmentId::new(&uuid_text(60)).expect("attachment id"),
        b"golf",
    )
    .await;
    let submission = SubmissionId::new(&uuid_text(61)).expect("submission id");
    seal_manifest(&store, &session, &submission, std::slice::from_ref(&bound)).await;
    store
        .commit_with_submission(
            acceptance(&session, 60),
            Some(SubmissionBinding {
                submission,
                attachments: vec![bound.clone()],
                subject: subject(),
            }),
        )
        .await
        .expect("bind attachment to session");

    // B：被保留消息的媒体引用保护的附件。
    let referenced = store.put(b"hotel", "image/png", at(1)).await.expect("put");
    let message = uuid_text(960);
    let pool = raw_write_pool(&dir.join("acp-remote.sqlite3")).await;
    sqlx::query(
        "INSERT INTO owned_message (message_id, session_id, turn_id, channel, state, \
         content_json, partial_reason, created_at, updated_at) \
         VALUES (?1, ?2, NULL, 'reply', 'completed', '[]', NULL, ?3, ?3)",
    )
    .bind(&message)
    .bind(session.as_str())
    .bind(at(1).as_str())
    .execute(&pool)
    .await
    .expect("message row");
    sqlx::query(
        "INSERT INTO owned_media_ref (message_id, position, attachment_id, kind) \
         VALUES (?1, 0, ?2, 'image')",
    )
    .bind(&message)
    .bind(referenced.id.as_str())
    .execute(&pool)
    .await
    .expect("media ref");
    pool.close().await;

    // C：无任何引用的孤儿，用来证明这一轮 GC 确实执行了（不是「什么都没删」）。
    let orphan = store.put(b"india", "text/plain", at(1)).await.expect("put");

    // at(50) = 2026-09-20T02；GC 截止点为 24 小时前的 2026-09-19T02，三个附件都已超过闲置窗口。
    let report = store.prune(policy(), at(50)).await.expect("prune idle");
    assert_eq!(report.removed_attachments, 1, "只有无引用的孤儿被回收");
    assert_eq!(report.freed_bytes, 5, "回收的字节数等于孤儿的大小");
    assert_eq!(table_count(&dir, "owned_attachment").await, 2);
    assert_eq!(table_count(&dir, "owned_attachment_link").await, 1);
    assert_eq!(table_count(&dir, "owned_media_ref").await, 1);
    assert!(
        store.get(&bound).await.expect("get").is_some(),
        "会话绑定仍保护附件"
    );
    assert!(
        store.get(&referenced.id).await.expect("get").is_some(),
        "消息媒体引用仍保护附件"
    );
    assert!(
        store.get(&orphan.id).await.expect("get").is_none(),
        "无引用附件按闲置窗口回收"
    );
    assert!(
        dir.join("attachments")
            .join(digest_of(b"golf").as_str())
            .exists()
    );
    assert!(dir.join(&referenced.relative_path).exists());
    assert!(!dir.join(&orphan.relative_path).exists());
    store.close().await;
}

/// 附件被按引用 GC 后，指向它的已封存传输同步变为 `expired`（不出现 sealed 但 `sealed: None`）。
#[tokio::test]
async fn gc_of_a_sealed_attachment_expires_its_transfer() {
    let dir = temp_dir("lifecycle-transfer-state");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    let transfer = AttachmentId::new(&uuid_text(4)).expect("attachment id");
    seal_bytes(&store, &session, &transfer, b"echo").await;
    assert_eq!(
        store
            .transfer(&transfer)
            .await
            .expect("read")
            .expect("row")
            .state,
        AttachmentTransferState::Sealed
    );

    let report = store.prune_lru(0, at(2)).await.expect("prune");
    assert_eq!(report.removed_attachments, 1);
    assert_eq!(table_count(&dir, "owned_attachment").await, 0);
    let row = store.transfer(&transfer).await.expect("read").expect("row");
    assert_eq!(row.state, AttachmentTransferState::Expired);
    assert!(row.sealed.is_none());
    // 重试封存会得到明确的「不是接收中」，而不是 `Corrupt`。
    let error = store
        .seal_transfer(TransferSeal {
            transfer: transfer.clone(),
            length: 4,
            sha256: digest_of(b"echo"),
            at: at(3),
        })
        .await
        .expect_err("expired transfer");
    assert!(matches!(
        error,
        acp_core::model::PortError::InvalidRequest(_)
    ));
    store.close().await;
}

/// 自定义 `storage.attachment_dir`：封存、清单复核、绑定与读取都走同一个目录映射。
#[tokio::test]
async fn custom_attachment_directory_is_used_end_to_end() {
    let data = temp_dir("lifecycle-custom-data");
    let blobs = temp_dir("lifecycle-custom-blobs");
    let mut config = StorageConfig::new(&data);
    config.attachment_dir = blobs.join("blobs");
    std::fs::create_dir_all(&config.attachment_dir).expect("blob dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(
            &config.attachment_dir,
            std::fs::Permissions::from_mode(0o700),
        )
        .expect("blob dir permissions");
    }
    let store = SqliteStore::open(config, &at(0)).await.expect("open");
    let session = create_session(&store).await;
    let attachment = seal_bytes(
        &store,
        &session,
        &AttachmentId::new(&uuid_text(5)).expect("attachment id"),
        b"foxtrot",
    )
    .await;
    let submission = SubmissionId::new(&uuid_text(13)).expect("submission id");
    seal_manifest(
        &store,
        &session,
        &submission,
        std::slice::from_ref(&attachment),
    )
    .await;
    store
        .commit_with_submission(
            acceptance(&session, 30),
            Some(SubmissionBinding {
                submission,
                attachments: vec![attachment.clone()],
                subject: subject(),
            }),
        )
        .await
        .expect("binding must find the bytes in the configured directory");
    // 文件确实落在自定义目录，而不是默认的 `<data_dir>/attachments`。
    assert!(
        blobs
            .join("blobs")
            .join(digest_of(b"foxtrot").as_str())
            .exists()
    );
    assert!(!data.join("attachments").exists());
    let chunk = store
        .read_chunk(&AttachmentRead {
            attachment,
            session,
            offset: 0,
            max_length: 1024,
        })
        .await
        .expect("read from the configured directory");
    assert_eq!(chunk.data, b"foxtrot");
    store.close().await;
}

/// 内容寻址去重让「今天重发 90 天前的同一文件」复用同一 `owned_attachment` 行，而该行保留**首次**
/// 上传的 `created_at`。prompt 受理时刚建立的会话绑定必须以「最近一次使用」判定保留期：同一轮
/// `prune` 不得删掉它，`read_chunk` 必须继续成功；消息保留期到期后才回收。
#[tokio::test]
async fn reused_content_bound_by_a_new_prompt_survives_prune() {
    let dir = temp_dir("lifecycle-reuse");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    let first_upload = Timestamp::new("2026-05-01T00:00:00.000Z").expect("timestamp");
    let first = seal_bytes_at(
        &store,
        &session,
        &AttachmentId::new(&uuid_text(6)).expect("attachment id"),
        b"hotel",
        first_upload,
    )
    .await;
    // 90 天后（今天）同一个文件再传一次：新传输、同一个附件行，只有 `last_used_at` 前进。
    let reused = seal_bytes_at(
        &store,
        &session,
        &AttachmentId::new(&uuid_text(7)).expect("attachment id"),
        b"hotel",
        at(2),
    )
    .await;
    assert_eq!(reused, first, "同样的字节只存一份");
    let submission = SubmissionId::new(&uuid_text(14)).expect("submission id");
    seal_manifest_at(
        &store,
        &session,
        &submission,
        std::slice::from_ref(&reused),
        at(2),
    )
    .await;
    store
        .commit_with_submission(
            acceptance(&session, 40),
            Some(SubmissionBinding {
                submission,
                attachments: vec![reused.clone()],
                subject: subject(),
            }),
        )
        .await
        .expect("accept with attachments");
    assert_eq!(table_count(&dir, "owned_attachment_link").await, 1);

    // 首轮清理与绑定同一时刻：链接与内容都还在，读取有授权。
    store.prune(policy(), at(2)).await.expect("prune");
    assert_eq!(
        table_count(&dir, "owned_attachment_link").await,
        1,
        "刚绑定的链接不得被同一轮清理删除"
    );
    assert_eq!(table_count(&dir, "owned_attachment").await, 1);
    assert!(
        dir.join("attachments")
            .join(digest_of(b"hotel").as_str())
            .exists(),
        "复用旧内容的消息在保留期内，文件不得被回收"
    );
    let chunk = store
        .read_chunk(&AttachmentRead {
            attachment: reused.clone(),
            session: session.clone(),
            offset: 0,
            max_length: 1024,
        })
        .await
        .expect("read after prune");
    assert_eq!(chunk.data, b"hotel");

    // 消息保留期（90 天）从最近一次绑定起算：到期后链接、行与文件一起回收。
    store.prune(policy(), later(1)).await.expect("prune");
    assert_eq!(table_count(&dir, "owned_attachment_link").await, 0);
    assert_eq!(table_count(&dir, "owned_attachment").await, 0);
    assert!(
        !dir.join("attachments")
            .join(digest_of(b"hotel").as_str())
            .exists()
    );
    store.close().await;
}

/// `AttachmentStore::link` 直接绑定旧内容（不经清单）：同样刷新「最近一次使用」，首轮清理不删链接。
#[tokio::test]
async fn direct_link_of_old_content_survives_prune() {
    let dir = temp_dir("lifecycle-link-refresh");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    let first_upload = Timestamp::new("2026-05-01T00:00:00.000Z").expect("timestamp");
    let reference = store
        .put(b"india", "application/octet-stream", first_upload)
        .await
        .expect("put");
    store
        .link(&session, &reference.id, AttachmentGeneration::new(1), at(2))
        .await
        .expect("link");

    store.prune(policy(), at(2)).await.expect("prune");
    assert_eq!(table_count(&dir, "owned_attachment_link").await, 1);
    assert_eq!(table_count(&dir, "owned_attachment").await, 1);
    let chunk = store
        .read_chunk(&AttachmentRead {
            attachment: reference.id.clone(),
            session: session.clone(),
            offset: 0,
            max_length: 1024,
        })
        .await
        .expect("read after prune");
    assert_eq!(chunk.data, b"india");

    store.prune(policy(), later(1)).await.expect("prune");
    assert_eq!(table_count(&dir, "owned_attachment_link").await, 0);
    assert_eq!(table_count(&dir, "owned_attachment").await, 0);
    store.close().await;
}
