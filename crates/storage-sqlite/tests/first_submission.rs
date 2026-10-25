//! 首条原子提交、派发围栏、启动收敛与准备附件暂存（`docs/CORE_PORTS_AND_STORAGE.md` §5.11/§7.3、
//! `design.md` D6.4/D7/D8）。

mod support;

use acp_core::model::{
    Actor, AgentId, AgentRef, AttachmentId, AttachmentSubject, ConnectionId, DeviceId, Digest,
    FirstSubmissionAdmission, FirstSubmissionConvergence, FirstSubmissionInput,
    FirstSubmissionSettlement, FirstSubmissionStage, FirstSubmissionWrite, ImmutableFileMetadata,
    MessageId, ModeId, ModeRef, Nonce, OriginEpoch, PortError, PreparationBinding,
    PromptContentBlock, PublicError, RequestId, RequestOutcome, ResourceGeneration, RuntimeEpoch,
    SessionId, StagedAbortReason, StagedChunk, StagedChunkOutcome, StagedManifestBegin,
    StagedManifestPage, StagedManifestSeal, StagedSeal, StagedSealOutcome, StagedTransferBegin,
    SubmissionId, Timestamp, TurnId,
};
use acp_core::model::{DispatchAdmission, DispatchFence};
use acp_core::ports::{FirstSubmission, PreparationAttachmentStaging};
use sqlx::SqlitePool;
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::{AT, digest_text, raw_pool, raw_write_pool, temp_dir};

fn ts(value: &str) -> Timestamp {
    Timestamp::new(value).expect("timestamp")
}

fn at() -> Timestamp {
    ts(AT)
}

fn later(millis: u64) -> Timestamp {
    at().checked_add_millis(millis).expect("timestamp")
}

fn uuid(seed: u64) -> String {
    format!("{seed:08x}-1111-4111-8111-{seed:012x}")
}

fn session(seed: u64) -> SessionId {
    SessionId::new(&uuid(seed)).expect("session")
}

fn request(seed: u64) -> RequestId {
    RequestId::new(&uuid(seed)).expect("request")
}

fn turn(seed: u64) -> TurnId {
    TurnId::new(&uuid(seed)).expect("turn")
}

fn message(seed: u64) -> MessageId {
    MessageId::new(&uuid(seed)).expect("message")
}

fn device(seed: u64) -> DeviceId {
    DeviceId::new(&uuid(seed)).expect("device")
}

fn connection(seed: u64) -> ConnectionId {
    ConnectionId::new(&uuid(seed)).expect("connection")
}

fn agent() -> AgentRef {
    AgentRef::try_new(AgentId::new("codex").expect("agent"), "Codex").expect("agent")
}

fn input(text: &str) -> FirstSubmissionInput {
    FirstSubmissionInput::new(vec![
        PromptContentBlock::from_json_text(&format!(
            r#"{{"type":"text","text":"{text}","future":{{"n":9007199254740993}}}}"#
        ))
        .expect("block"),
    ])
    .expect("input")
}

fn submission_write(seed: u64, text: &str) -> FirstSubmissionWrite {
    FirstSubmissionWrite {
        actor: Actor::Device {
            device: device(900),
        },
        session: session(seed),
        first_submission_id: acp_core::model::FirstSubmissionId::new(&uuid(seed + 1000))
            .expect("first submission"),
        user_message_id: message(seed + 2000),
        origin_epoch: OriginEpoch::new(&uuid(seed + 3000)).expect("origin epoch"),
        agent: agent(),
        workspace_cwd: None,
        workspace_directory: None,
        mode: Some(ModeRef::try_new(ModeId::new("code").expect("mode"), "Code").expect("mode ref")),
        config: Vec::new(),
        input: input(text),
        manifest: None,
        request_id: request(seed + 4000),
        request_fingerprint: Digest::new(&digest_text(&format!("request-{seed}"))).expect("digest"),
        turn: turn(seed + 5000),
        at: at(),
    }
}

async fn counts(pool: &SqlitePool) -> (i64, i64, i64, i64, i64, i64, i64) {
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_session")
        .fetch_one(pool)
        .await
        .expect("sessions");
    let messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_message")
        .fetch_one(pool)
        .await
        .expect("messages");
    let turns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_turn")
        .fetch_one(pool)
        .await
        .expect("turns");
    let commands: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_command")
        .fetch_one(pool)
        .await
        .expect("commands");
    let submissions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_first_submission")
        .fetch_one(pool)
        .await
        .expect("submissions");
    let attachments: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_attachment")
        .fetch_one(pool)
        .await
        .expect("attachments");
    let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_attachment_link")
        .fetch_one(pool)
        .await
        .expect("links");
    (
        sessions,
        messages,
        turns,
        commands,
        submissions,
        attachments,
        links,
    )
}

async fn open(dir: &support::TempDir) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at())
        .await
        .expect("open store")
}

#[tokio::test]
async fn first_submission_writes_everything_and_is_readable_without_echo() {
    let dir = temp_dir("first-submission-all");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");
    let write = submission_write(1, "hello");
    let expected_message = write.user_message_id.clone();
    let expected_text = write.input.json().as_str().to_owned();
    let admission = store
        .commit_first_submission(write.clone())
        .await
        .expect("commit");
    let FirstSubmissionAdmission::Accepted(record) = admission else {
        panic!("expected accepted");
    };
    assert_eq!(record.stage, FirstSubmissionStage::SavedNotDispatched);
    assert!(!record.ever_dispatched);
    assert_eq!(record.session, write.session);

    // 正文不依赖 Agent 回显：直接读 user 通道。
    let first = store
        .first_user_message(write.session.clone())
        .await
        .expect("read")
        .expect("first message");
    assert_eq!(first.message, expected_message);
    assert_eq!(first.turn.as_ref(), Some(&write.turn));
    assert_eq!(first.content.as_str(), expected_text);

    let pool = raw_pool(&path).await;
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1, 0, 0));
    let (channel, state): (String, String) =
        sqlx::query_as("SELECT channel, state FROM owned_message WHERE message_id = ?1")
            .bind(expected_message.as_str())
            .fetch_one(&pool)
            .await
            .expect("message row");
    assert_eq!((channel.as_str(), state.as_str()), ("user", "completed"));
    let stage: String = sqlx::query_scalar("SELECT stage FROM owned_first_submission")
        .fetch_one(&pool)
        .await
        .expect("stage");
    assert_eq!(stage, "saved_not_dispatched");
    pool.close().await;

    // 重复调用返回既有记录，不追加第二份首条。
    let again = store
        .commit_first_submission(write)
        .await
        .expect("idempotent");
    assert!(matches!(
        again,
        FirstSubmissionAdmission::AlreadyCommitted(_)
    ));
    let pool = raw_pool(&path).await;
    assert_eq!(counts(&pool).await, (1, 1, 1, 1, 1, 0, 0));
    pool.close().await;
    store.close().await;
}

#[tokio::test]
async fn a_failed_step_leaves_no_partial_rows() {
    let dir = temp_dir("first-submission-rollback");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");
    // 先造一个已存在的会话与消息，占用目标 user_message_id（唯一列）。
    let pool = raw_write_pool(&path).await;
    sqlx::query(
        "INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, origin_epoch, \
         version, created_at, updated_at) VALUES (?1, NULL, 'agent', 'Agent', 'idle', ?2, 1, ?3, ?3)",
    )
    .bind(session(77).as_str())
    .bind(uuid(78))
    .bind(AT)
    .execute(&pool)
    .await
    .expect("seed session");
    sqlx::query(
        "INSERT INTO owned_message (message_id, session_id, turn_id, channel, state, content_json, \
         created_at, updated_at) VALUES (?1, ?2, NULL, 'reply', 'completed', '{}', ?3, ?3)",
    )
    .bind(message(2001).as_str())
    .bind(session(77).as_str())
    .bind(AT)
    .execute(&pool)
    .await
    .expect("seed message");
    pool.close().await;

    let mut conflicting = submission_write(1, "hello");
    conflicting.user_message_id = message(2001);
    let error = store
        .commit_first_submission(conflicting)
        .await
        .expect_err("unique message id must fail the transaction");
    assert!(matches!(
        error,
        PortError::Backend(_) | PortError::Conflict(_)
    ));

    let pool = raw_pool(&path).await;
    assert_eq!(counts(&pool).await, (1, 1, 0, 0, 0, 0, 0));
    let session_1: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_session WHERE session_id = ?1")
            .bind(session(1).as_str())
            .fetch_one(&pool)
            .await
            .expect("session 1");
    assert_eq!(session_1, 0, "失败步骤之前的会话行也必须回滚");
    pool.close().await;
    store.close().await;
}

fn binding(seed: u64) -> PreparationBinding {
    PreparationBinding {
        device: device(900),
        session: session(seed),
        tab_nonce: Nonce::new(&"A".repeat(43)).expect("nonce"),
        runtime_epoch: RuntimeEpoch::new(&uuid(seed + 6000)).expect("runtime epoch"),
        generation: ResourceGeneration::new(1),
        claims_digest: Digest::new(&digest_text("claims")).expect("digest"),
    }
}

fn manifest_begin(seed: u64, count: u32) -> StagedManifestBegin {
    StagedManifestBegin {
        submission: SubmissionId::new(&uuid(seed + 7000)).expect("submission"),
        binding: binding(seed),
        subject: AttachmentSubject::Device(device(900)),
        connection: connection(seed + 8000),
        count,
        at: at(),
    }
}

async fn stage_one(store: &SqliteStore, seed: u64, bytes: &[u8]) -> (AttachmentId, Digest) {
    stage_one_as(store, seed, seed, bytes).await
}

/// 同一绑定（`seed`）下的另一条传输（`transfer_seed` 区分传输标识）：一份清单可以引用多个已封存项。
async fn stage_one_as(
    store: &SqliteStore,
    seed: u64,
    transfer_seed: u64,
    bytes: &[u8],
) -> (AttachmentId, Digest) {
    let transfer = AttachmentId::new(&uuid(transfer_seed + 9000)).expect("transfer");
    let begin = StagedTransferBegin {
        transfer: transfer.clone(),
        binding: binding(seed),
        subject: AttachmentSubject::Device(device(900)),
        connection: connection(seed + 8000),
        total_length: bytes.len() as u64,
        immutable: ImmutableFileMetadata {
            media_type: "text/plain".to_owned(),
            file_name: "note.txt".to_owned(),
        },
        at: at(),
    };
    store.begin(begin).await.expect("begin");
    let next = store
        .append(StagedChunk {
            transfer: transfer.clone(),
            connection: connection(seed + 8000),
            offset: 0,
            bytes: bytes.to_vec(),
            at: at(),
        })
        .await
        .expect("append");
    assert!(matches!(next, StagedChunkOutcome::Accepted { .. }));
    // 用真实字节摘要封存（`stage_one` 只接受与其内容一致的摘要）。
    let real = {
        use base64::Engine as _;
        use sha2::Digest as _;
        Digest::new(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(bytes)),
        )
        .expect("digest")
    };
    match store
        .seal(StagedSeal {
            transfer: transfer.clone(),
            connection: connection(seed + 8000),
            length: bytes.len() as u64,
            sha256: real.clone(),
            at: at(),
        })
        .await
        .expect("seal")
    {
        StagedSealOutcome::Sealed(facts) => {
            assert_eq!(facts.sha256, real);
            (transfer, real)
        }
        other => panic!("expected Sealed, got {other:?}"),
    }
}

async fn seal_manifest(
    store: &SqliteStore,
    seed: u64,
    transfers: Vec<AttachmentId>,
) -> SubmissionId {
    let begin = manifest_begin(seed, transfers.len() as u32);
    let submission = begin.submission.clone();
    store.begin_manifest(begin).await.expect("begin manifest");
    store
        .append_manifest_page(StagedManifestPage {
            submission: submission.clone(),
            connection: connection(seed + 8000),
            transfers,
            final_page: true,
            at: at(),
        })
        .await
        .expect("page");
    store
        .seal_manifest(StagedManifestSeal {
            submission: submission.clone(),
            connection: connection(seed + 8000),
            at: at(),
        })
        .await
        .expect("seal manifest");
    submission
}

#[tokio::test]
async fn staging_has_no_database_rows_and_claim_is_atomic() {
    let dir = temp_dir("staging-no-rows");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");
    let (transfer, sha) = stage_one(&store, 1, b"attachment bytes").await;
    let submission = seal_manifest(&store, 1, vec![transfer]).await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        counts(&pool).await,
        (0, 0, 0, 0, 0, 0, 0),
        "暂存不得写任何 DB 行"
    );
    let transfers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_attachment_transfer")
        .fetch_one(&pool)
        .await
        .expect("transfers");
    assert_eq!(transfers, 0);
    pool.close().await;

    let mut write = submission_write(1, "with attachment");
    write.manifest = Some(acp_core::model::SubmissionManifestClaim {
        submission,
        runtime_epoch: binding(1).runtime_epoch,
        generation: ResourceGeneration::new(1),
    });
    let admission = store.commit_first_submission(write).await.expect("commit");
    assert!(matches!(admission, FirstSubmissionAdmission::Accepted(_)));

    let pool = raw_pool(&path).await;
    let (attachments, links, media): (i64, i64, i64) = (
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_attachment")
            .fetch_one(&pool)
            .await
            .expect("attachments"),
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_attachment_link")
            .fetch_one(&pool)
            .await
            .expect("links"),
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_media_ref")
            .fetch_one(&pool)
            .await
            .expect("media"),
    );
    assert_eq!((attachments, links, media), (1, 1, 1));
    let stored_sha: String = sqlx::query_scalar("SELECT sha256 FROM owned_attachment")
        .fetch_one(&pool)
        .await
        .expect("sha");
    assert_eq!(stored_sha, sha.as_str());
    pool.close().await;
    assert!(
        dir.join("attachments").join(sha.as_str()).is_file(),
        "认领后字节位于正式内容寻址路径"
    );
    store.close().await;
}

#[tokio::test]
async fn dispatch_fence_is_cas_and_continue_needs_a_failed_attempt() {
    let dir = temp_dir("dispatch-fence");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");
    let write = submission_write(1, "hello");
    let session_id = write.session.clone();
    let first_submission_id = write.first_submission_id.clone();
    let first_request = write.request_id.clone();
    let first_turn = write.turn.clone();
    store.commit_first_submission(write).await.expect("commit");

    let fence = |request_seed: u64, retry: Option<RequestId>| DispatchFence {
        session: session_id.clone(),
        first_submission_id: first_submission_id.clone(),
        actor: Actor::Device {
            device: device(900),
        },
        request_id: request(request_seed),
        retry_of: retry,
        request_fingerprint: Digest::new(&digest_text("request-1")).expect("digest"),
        agent_session: acp_core::model::AgentSessionId::new("agent-session").expect("agent"),
        resource_generation: ResourceGeneration::new(1),
        at: later(1),
    };
    assert_eq!(
        store
            .begin_dispatch(fence(4001, None))
            .await
            .expect("dispatch"),
        DispatchAdmission::Granted
    );
    assert_eq!(
        store
            .begin_dispatch(fence(4001, None))
            .await
            .expect("repeat"),
        DispatchAdmission::NotEligible,
        "同一次尝试不得二次跨越派发边界"
    );
    let record = store
        .first_submission(session_id.clone())
        .await
        .expect("read")
        .expect("record");
    assert_eq!(record.stage, FirstSubmissionStage::DispatchStarted);
    assert!(record.ever_dispatched);
    assert_eq!(
        record.agent_session.as_ref().map(|id| id.as_str()),
        Some("agent-session")
    );

    // 已派发后完成：终态不可再派发。
    let settled = store
        .settle_first_submission(FirstSubmissionSettlement {
            session: session_id.clone(),
            actor: Actor::Device {
                device: device(900),
            },
            request_id: first_request.clone(),
            request_fingerprint: Digest::new(&digest_text("request-4001")).expect("digest"),
            outcome: RequestOutcome::Completed,
            error: None,
            at: later(2),
        })
        .await
        .expect("settle");
    assert_eq!(settled.stage, FirstSubmissionStage::Terminal);
    assert_eq!(
        settled.terminal.as_ref().map(|terminal| terminal.outcome),
        Some(RequestOutcome::Completed)
    );
    assert_eq!(
        store
            .begin_dispatch(fence(4002, Some(first_request.clone())))
            .await
            .expect("after terminal"),
        DispatchAdmission::NotEligible
    );

    // 新会话：可证明未派发的失败保留正文，显式继续使用新请求且不新增首条。
    let write = submission_write(2, "retry me");
    let retry_session = write.session.clone();
    let retry_first_submission = write.first_submission_id.clone();
    let retry_first_request = write.request_id.clone();
    let retry_turn = write.turn.clone();
    let user_message = write.user_message_id.clone();
    store
        .commit_first_submission(write)
        .await
        .expect("commit second");
    let failed = store
        .settle_first_submission(FirstSubmissionSettlement {
            session: retry_session.clone(),
            actor: Actor::Device {
                device: device(900),
            },
            request_id: retry_first_request.clone(),
            request_fingerprint: Digest::new(&digest_text("request-2")).expect("digest"),
            outcome: RequestOutcome::Failed,
            error: Some(
                PublicError::coded("internal.unavailable", "endpoint failed", true).expect("error"),
            ),
            at: later(3),
        })
        .await
        .expect("fail before dispatch");
    assert_eq!(failed.stage, FirstSubmissionStage::SavedNotDispatched);
    assert!(!failed.ever_dispatched);
    assert_eq!(
        failed.terminal.as_ref().map(|terminal| terminal.outcome),
        Some(RequestOutcome::Failed)
    );

    // 继续必须引用本次失败的请求；新请求 ID 才允许再次派发。
    assert_eq!(
        store
            .begin_dispatch(DispatchFence {
                session: retry_session.clone(),
                first_submission_id: retry_first_submission.clone(),
                actor: Actor::Device {
                    device: device(900)
                },
                request_id: request(4012),
                retry_of: None,
                request_fingerprint: Digest::new(&digest_text("request-4012")).expect("digest"),
                agent_session: acp_core::model::AgentSessionId::new("agent-session")
                    .expect("agent"),
                resource_generation: ResourceGeneration::new(1),
                at: later(4),
            })
            .await
            .expect("no retry ref"),
        DispatchAdmission::NotEligible
    );
    assert_eq!(
        store
            .begin_dispatch(DispatchFence {
                session: retry_session.clone(),
                first_submission_id: retry_first_submission,
                actor: Actor::Device {
                    device: device(900)
                },
                request_id: request(4013),
                retry_of: Some(retry_first_request),
                request_fingerprint: Digest::new(&digest_text("request-4013")).expect("digest"),
                agent_session: acp_core::model::AgentSessionId::new("agent-session")
                    .expect("agent"),
                resource_generation: ResourceGeneration::new(1),
                at: later(5),
            })
            .await
            .expect("continue"),
        DispatchAdmission::Granted
    );
    let pool = raw_pool(&path).await;
    let first_message: (String, String) =
        sqlx::query_as("SELECT message_id, channel FROM owned_message WHERE session_id = ?1")
            .bind(retry_session.as_str())
            .fetch_one(&pool)
            .await
            .expect("only one message");
    assert_eq!(first_message.0, user_message.as_str());
    assert_eq!(first_message.1, "user");
    let turn_state: String = sqlx::query_scalar("SELECT state FROM owned_turn WHERE turn_id = ?1")
        .bind(retry_turn.as_str())
        .fetch_one(&pool)
        .await
        .expect("turn");
    assert_eq!(turn_state, "running");
    let _ = first_turn;
    pool.close().await;
    store.close().await;
}

/// §6 第 16 条 + D8：`unsettled_commands` 不返回首条提交尝试的命令——那是 `converge_first_submission`
/// 的独占结算对象。
///
/// 两条 writer 必须分工：恢复结算普通 `accepted` mutation，收敛结算首个/当前首条尝试。恢复若抢先把它
/// 写成 `uncertain`，收敛再结算同一条命令会得到 `Conflict(Consumed)`，启动因此中止。排除必须按
/// `(actor, requestId)` 精确匹配——同一会话上的普通 mutation 仍要交给启动恢复。
///
/// 变红点：把首条 `session.start` 也列进恢复 ⇒ `unsettled` 非空；排除写成「同会话一律不返回」⇒
/// 对照的普通 mutation 消失；收敛不再写终态 ⇒ 命令停在 `accepted`。
#[tokio::test]
async fn unsettled_commands_leaves_first_submission_attempts_to_convergence() {
    use acp_core::model::{CommandKind, CommandStatus};
    use acp_core::ports::{IdempotencyRecord, OwnedCommit, ReplayLimit, SessionStore};

    let dir = temp_dir("unsettled-first-submission");
    let store = open(&dir).await;
    let write = submission_write(5, "saved before dispatch");
    let session_id = write.session.clone();
    let first_request = write.request_id.clone();
    let actor = Actor::Device {
        device: device(900),
    };
    store.commit_first_submission(write).await.expect("commit");

    // 崩溃窗口的形状：`session.start` 已受理、没有终态事件。
    let accepted = store
        .find_request(&first_request, &actor)
        .await
        .expect("find")
        .expect("受理行必须落盘");
    assert_eq!(accepted.status(), CommandStatus::Accepted);
    assert!(accepted.terminal_event().is_none());

    // 对照：同一会话上一条普通 mutation 仍在启动恢复的输入里。
    let other_request = request(7000);
    store
        .commit(OwnedCommit {
            session: Some(session_id.clone()),
            at: later(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: actor.clone(),
                request: other_request.clone(),
                command: "session.prompt".to_owned(),
                kind: CommandKind::Mutation,
                session: Some(session_id.clone()),
                expected_version: None,
                request_fingerprint: Digest::new(&digest_text("other-request")).expect("digest"),
                accepted_at: later(2),
            }),
            command_terminal: None,
            origin_epoch: None,
            input: None,
        })
        .await
        .expect("accept a plain mutation");

    let unsettled = store
        .unsettled_commands(ReplayLimit::new(16))
        .await
        .expect("unsettled");
    assert_eq!(
        unsettled
            .iter()
            .map(|row| row.request().as_str())
            .collect::<Vec<_>>(),
        vec![other_request.as_str()],
        "只有首条尝试被排除；普通 accepted mutation 仍交给启动恢复"
    );

    // 收敛是唯一结算者：写 failed、保留可继续的阶段，命令不再停留在 accepted。
    let convergence = store
        .converge_first_submission(session_id.clone(), later(3))
        .await
        .expect("converge");
    let FirstSubmissionConvergence::ProvablyNotDispatched(record) = convergence else {
        panic!("保存后崩溃必须收敛为可证明未派发");
    };
    assert_eq!(record.stage, FirstSubmissionStage::SavedNotDispatched);
    let settled = store
        .find_request(&first_request, &actor)
        .await
        .expect("find")
        .expect("受理行仍存在");
    assert_eq!(settled.status(), CommandStatus::Failed);
    assert_eq!(
        settled.error().map(|error| error.code()),
        Some("internal.unavailable")
    );
    assert!(settled.terminal_event().is_some());

    // 显式继续跨越派发围栏后，当前尝试同样是一条 `accepted` 且无终态的命令：它也必须留在启动
    // 恢复之外（排除按 `current_request_id` 也要命中，不能只认首个请求）。
    let continue_request = request(7100);
    assert_eq!(
        store
            .begin_dispatch(DispatchFence {
                session: session_id.clone(),
                first_submission_id: record.first_submission_id.clone(),
                actor: actor.clone(),
                request_id: continue_request.clone(),
                retry_of: Some(first_request.clone()),
                request_fingerprint: Digest::new(&digest_text("continue-request")).expect("digest"),
                agent_session: acp_core::model::AgentSessionId::new("agent-session")
                    .expect("agent session"),
                resource_generation: ResourceGeneration::new(1),
                at: later(4),
            })
            .await
            .expect("dispatch the continue"),
        DispatchAdmission::Granted
    );
    let dispatched = store
        .find_request(&continue_request, &actor)
        .await
        .expect("find")
        .expect("继续尝试的受理行必须落盘");
    assert_eq!(dispatched.status(), CommandStatus::Accepted);
    assert!(dispatched.terminal_event().is_none());
    let unsettled = store
        .unsettled_commands(ReplayLimit::new(16))
        .await
        .expect("unsettled");
    assert_eq!(
        unsettled
            .iter()
            .map(|row| row.request().as_str())
            .collect::<Vec<_>>(),
        vec![other_request.as_str()],
        "当前尝试（current_request_id）同样归收敛结算，恢复不得看到它"
    );

    store.close().await;
}

#[tokio::test]
async fn crash_windows_converge_without_automatic_dispatch() {
    let dir = temp_dir("crash-convergence");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");

    // 窗口 1：保存成功、未写 dispatch_started → 明确失败，等待显式继续。
    let write = submission_write(3, "saved only");
    let session_id = write.session.clone();
    let request_id = write.request_id.clone();
    store.commit_first_submission(write).await.expect("commit");
    let convergence = store
        .converge_first_submission(session_id.clone(), later(10))
        .await
        .expect("converge");
    let FirstSubmissionConvergence::ProvablyNotDispatched(record) = convergence else {
        panic!("expected provably-not-dispatched");
    };
    assert_eq!(record.stage, FirstSubmissionStage::SavedNotDispatched);
    assert!(!record.ever_dispatched);
    assert_eq!(
        record.terminal.as_ref().map(|terminal| terminal.outcome),
        Some(RequestOutcome::Failed)
    );
    let pool = raw_pool(&path).await;
    let status: String =
        sqlx::query_scalar("SELECT status FROM owned_command WHERE request_id = ?1")
            .bind(request_id.as_str())
            .fetch_one(&pool)
            .await
            .expect("status");
    assert_eq!(status, "failed");
    pool.close().await;

    // 窗口 2：dispatch_started 后崩溃 → uncertain，不再自动派发。
    let write = submission_write(4, "maybe dispatched");
    let session_id = write.session.clone();
    let first_submission = write.first_submission_id.clone();
    let first_request = write.request_id.clone();
    store.commit_first_submission(write).await.expect("commit");
    assert_eq!(
        store
            .begin_dispatch(DispatchFence {
                session: session_id.clone(),
                first_submission_id: first_submission,
                actor: Actor::Device {
                    device: device(900)
                },
                request_id: first_request.clone(),
                retry_of: None,
                request_fingerprint: Digest::new(&digest_text("request-4")).expect("digest"),
                agent_session: acp_core::model::AgentSessionId::new("agent-session")
                    .expect("agent"),
                resource_generation: ResourceGeneration::new(1),
                at: later(1),
            })
            .await
            .expect("dispatch"),
        DispatchAdmission::Granted
    );
    let convergence = store
        .converge_first_submission(session_id.clone(), later(11))
        .await
        .expect("converge");
    let FirstSubmissionConvergence::Uncertain(record) = convergence else {
        panic!("expected uncertain");
    };
    assert_eq!(record.stage, FirstSubmissionStage::Terminal);
    assert!(record.ever_dispatched);
    assert_eq!(
        store
            .converge_first_submission(session_id.clone(), later(12))
            .await
            .expect("second converge"),
        FirstSubmissionConvergence::Terminal(record.clone())
    );
    let _ = session_id;
    store.close().await;
}

#[tokio::test]
async fn staging_orphans_are_reclaimed_after_the_window_and_restart() {
    let dir = temp_dir("staging-orphans");
    let store = open(&dir).await;
    let (transfer, _sha) = stage_one(&store, 2, b"unclaimed").await;
    assert!(
        store
            .status(transfer.clone())
            .await
            .expect("status")
            .is_some()
    );
    // 窗口内不删。
    assert_eq!(
        store.release_orphans(later(1)).await.expect("sweep fresh"),
        0
    );
    // 超过 24 小时：元数据与字节一起回收。
    let before = ts("2100-01-01T00:00:00.000Z");
    let removed = store.release_orphans(before).await.expect("sweep");
    assert!(removed >= 1);
    assert!(store.status(transfer).await.expect("status").is_none());
    store.close().await;

    // 重启（运行期元数据丢失）后，启动/24 小时扫描仍能回收磁盘上的孤儿字节。
    let store = open(&dir).await;
    let (transfer, _sha) = stage_one(&store, 3, b"crash orphan").await;
    let _ = transfer;
    store.close().await;
    let reopen_at = ts("2100-01-01T00:00:00.000Z");
    let store = SqliteStore::open(StorageConfig::new(&dir), &reopen_at)
        .await
        .expect("reopen");
    let staging_dir = dir.join("attachments").join(".preparation");
    let bytes = std::fs::read_dir(&staging_dir)
        .map(|entries| entries.flatten().count())
        .unwrap_or(0);
    assert_eq!(
        bytes, 2,
        "receiving/sealed 两个子目录仍在；孤儿文件应已删除"
    );
    let files = walk_files(&staging_dir);
    assert!(files.is_empty(), "重启后孤儿字节被回收: {files:?}");
    store.close().await;
}

fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[tokio::test]
async fn abort_removes_only_unclaimed_staging_state() {
    let dir = temp_dir("staging-abort");
    let store = open(&dir).await;
    let transfer = AttachmentId::new(&uuid(9991)).expect("transfer");
    store
        .begin(StagedTransferBegin {
            transfer: transfer.clone(),
            binding: binding(5),
            subject: AttachmentSubject::Device(device(900)),
            connection: connection(5),
            total_length: 4,
            immutable: ImmutableFileMetadata {
                media_type: "text/plain".to_owned(),
                file_name: "a.txt".to_owned(),
            },
            at: at(),
        })
        .await
        .expect("begin");
    store
        .abort(transfer.clone(), StagedAbortReason::ClientCancelled)
        .await
        .expect("abort");
    assert!(store.status(transfer).await.expect("status").is_none());
    store.close().await;
}

fn days_later(days: u64) -> Timestamp {
    later(days * 86_400_000)
}

fn sealed_bytes(dir: &std::path::Path, sha: &Digest) -> std::path::PathBuf {
    dir.join("attachments")
        .join(".preparation")
        .join("sealed")
        .join(sha.as_str())
}

fn owned_bytes(dir: &std::path::Path, sha: &Digest) -> std::path::PathBuf {
    dir.join("attachments").join(sha.as_str())
}

fn fence(
    session: &SessionId,
    first_submission: &acp_core::model::FirstSubmissionId,
    request_id: RequestId,
    retry_of: Option<RequestId>,
    fingerprint: &str,
) -> DispatchFence {
    DispatchFence {
        session: session.clone(),
        first_submission_id: first_submission.clone(),
        actor: Actor::Device {
            device: device(900),
        },
        request_id,
        retry_of,
        request_fingerprint: Digest::new(&digest_text(fingerprint)).expect("digest"),
        agent_session: acp_core::model::AgentSessionId::new("agent-session").expect("agent"),
        resource_generation: ResourceGeneration::new(1),
        at: later(1),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settlement_projects_the_frozen_result_and_keeps_one_terminal_event() {
    let dir = temp_dir("first-submission-result");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");
    let write = submission_write(1, "hello");
    let session_id = write.session.clone();
    let first_submission_id = write.first_submission_id.clone();
    let first_request = write.request_id.clone();
    let user_message = write.user_message_id.clone();
    let turn = write.turn.clone();
    store.commit_first_submission(write).await.expect("commit");

    // 初始事件（D7）：首条用户消息在事件流里可见，带来源标记与 TTL（short_term → 7 天）。
    let pool = raw_pool(&path).await;
    let (event_type, payload, expires): (String, String, Option<String>) = sqlx::query_as(
        "SELECT event_type, payload_json, expires_at FROM owned_event \
         WHERE session_id = ?1 AND event_type = 'user.message.delta'",
    )
    .bind(session_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("initial event");
    assert_eq!(event_type, "user.message.delta");
    let initial: serde_json::Value = serde_json::from_str(&payload).expect("initial view");
    assert_eq!(initial["messageId"], user_message.as_str());
    assert_eq!(initial["turnId"], turn.as_str());
    assert_eq!(initial["deltaIndex"], "0");
    assert_eq!(initial["source"], "owned_first_submission");
    assert_eq!(expires.as_deref(), Some(days_later(7).as_str()));
    pool.close().await;

    // 派发后完成：终态必须带冻结的六字段投影，并持久化到 `result_json`。
    assert_eq!(
        store
            .begin_dispatch(fence(
                &session_id,
                &first_submission_id,
                first_request.clone(),
                None,
                "request-1",
            ))
            .await
            .expect("dispatch"),
        DispatchAdmission::Granted
    );
    let settlement = FirstSubmissionSettlement {
        session: session_id.clone(),
        actor: Actor::Device {
            device: device(900),
        },
        request_id: first_request.clone(),
        request_fingerprint: Digest::new(&digest_text("request-1")).expect("digest"),
        outcome: RequestOutcome::Completed,
        error: None,
        at: later(2),
    };
    let settled = store
        .settle_first_submission(settlement.clone())
        .await
        .expect("settle");
    assert_eq!(settled.stage, FirstSubmissionStage::Terminal);
    assert_eq!(
        settled.terminal.as_ref().map(|terminal| terminal.outcome),
        Some(RequestOutcome::Completed)
    );

    let pool = raw_pool(&path).await;
    let (status, result_json, terminal_event_id): (String, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT status, result_json, terminal_event_id FROM owned_command WHERE request_id = ?1",
        )
        .bind(first_request.as_str())
        .fetch_one(&pool)
        .await
        .expect("command row");
    assert_eq!(status, "completed");
    let result: serde_json::Value =
        serde_json::from_str(&result_json.expect("completed 必须持久化 result_json"))
            .expect("result json");
    assert_eq!(result["stage"], "terminal");
    assert_eq!(result["firstSubmissionId"], first_submission_id.as_str());
    assert_eq!(result["firstRequestId"], first_request.as_str());
    assert_eq!(result["currentRequestId"], first_request.as_str());
    assert_eq!(result["everDispatched"], true);
    assert_eq!(result["canContinue"], false);

    let (event_id, payload, expires): (String, String, Option<String>) = sqlx::query_as(
        "SELECT event_id, payload_json, expires_at FROM owned_event \
         WHERE session_id = ?1 AND event_type = 'command.completed'",
    )
    .bind(session_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("one terminal event");
    assert_eq!(Some(event_id.as_str()), terminal_event_id.as_deref());
    let completed: serde_json::Value = serde_json::from_str(&payload).expect("terminal view");
    assert_eq!(completed["requestId"], first_request.as_str());
    assert_eq!(
        completed.get("result"),
        Some(&result),
        "事件里的 result 与 result_json 同源"
    );
    // durable 终态走 `transcript_retention_days`（默认 90 天），窗口从结算时刻算起。
    assert_eq!(
        expires.as_deref(),
        Some(later(2 + 90 * 86_400_000).as_str())
    );
    // 领域事件先于命令终态（§11.2 的 sequence 顺序）。
    let order: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM owned_event WHERE session_id = ?1 ORDER BY session_sequence ASC",
    )
    .bind(session_id.as_str())
    .fetch_all(&pool)
    .await
    .expect("events");
    assert_eq!(
        order,
        vec!["user.message.delta", "turn.completed", "command.completed"]
    );
    pool.close().await;

    // 重复结算与并发结算：同一 requestId 只读原终态，不追加事件、不改 current_request_id。
    let repeated = store
        .settle_first_submission(settlement.clone())
        .await
        .expect("replay");
    assert_eq!(
        repeated.terminal.as_ref().map(|terminal| terminal.outcome),
        Some(RequestOutcome::Completed)
    );
    let (first, second) = tokio::join!(
        store.settle_first_submission(settlement.clone()),
        store.settle_first_submission(settlement)
    );
    assert!(first.is_ok(), "并发重放必须返回原终态: {first:?}");
    assert!(second.is_ok(), "并发重放必须返回原终态: {second:?}");

    let pool = raw_pool(&path).await;
    let terminals: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM owned_event WHERE event_type IN \
         ('command.completed','command.failed','command.uncertain')",
    )
    .fetch_one(&pool)
    .await
    .expect("terminals");
    assert_eq!(
        terminals, 1,
        "同一个 request 只能有一个 command terminal event"
    );
    let current: String = sqlx::query_scalar(
        "SELECT current_request_id FROM owned_first_submission WHERE session_id = ?1",
    )
    .bind(session_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("current request");
    assert_eq!(
        current,
        first_request.as_str(),
        "重放不得改写 current_request_id"
    );
    let completed_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_event WHERE event_type = 'turn.completed'")
            .fetch_one(&pool)
            .await
            .expect("turn events");
    assert_eq!(completed_count, 1, "领域事件也不得重复");
    pool.close().await;
    store.close().await;
}

#[tokio::test]
async fn provably_not_dispatched_failure_stays_retryable_and_writes_one_terminal() {
    let dir = temp_dir("first-submission-failed");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");
    let write = submission_write(2, "endpoint failed");
    let session_id = write.session.clone();
    let first_request = write.request_id.clone();
    store.commit_first_submission(write).await.expect("commit");

    let error = PublicError::coded("internal.unavailable", "endpoint failed", true).expect("error");
    let settlement = FirstSubmissionSettlement {
        session: session_id.clone(),
        actor: Actor::Device {
            device: device(900),
        },
        request_id: first_request.clone(),
        request_fingerprint: Digest::new(&digest_text("request-2")).expect("digest"),
        outcome: RequestOutcome::Failed,
        error: Some(error),
        at: later(3),
    };
    let settled = store
        .settle_first_submission(settlement.clone())
        .await
        .expect("fail before dispatch");
    assert_eq!(settled.stage, FirstSubmissionStage::SavedNotDispatched);
    assert!(!settled.ever_dispatched);

    // 可证明未派发的失败没有 turn 终态事件（turn 仍 queued），也没有 result。
    let pool = raw_pool(&path).await;
    let failed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_event WHERE event_type = 'command.failed'")
            .fetch_one(&pool)
            .await
            .expect("failed events");
    assert_eq!(failed, 1);
    let turn_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_event WHERE event_type = 'turn.failed'")
            .fetch_one(&pool)
            .await
            .expect("turn events");
    assert_eq!(turn_events, 0, "未派发的失败保留 queued turn");
    let result_json: Option<String> =
        sqlx::query_scalar("SELECT result_json FROM owned_command WHERE request_id = ?1")
            .bind(first_request.as_str())
            .fetch_one(&pool)
            .await
            .expect("result");
    assert!(result_json.is_none(), "failed 不带 result");
    pool.close().await;

    // 重复结算（含不同声明结果）只读原终态。
    for outcome in [RequestOutcome::Failed, RequestOutcome::Completed] {
        let replay = store
            .settle_first_submission(FirstSubmissionSettlement {
                outcome,
                ..settlement.clone()
            })
            .await
            .expect("replay");
        assert_eq!(
            replay.terminal.as_ref().map(|terminal| terminal.outcome),
            Some(RequestOutcome::Failed)
        );
    }
    let pool = raw_pool(&path).await;
    let failed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_event WHERE event_type = 'command.failed'")
            .fetch_one(&pool)
            .await
            .expect("failed events");
    assert_eq!(failed, 1);
    pool.close().await;

    // 缺少 error 的 failed/uncertain 结算被显式拒绝（不写半条终态）。
    let missing = store
        .settle_first_submission(FirstSubmissionSettlement {
            session: session_id,
            actor: Actor::Device {
                device: device(900),
            },
            request_id: request(9001),
            request_fingerprint: Digest::new(&digest_text("request-9001")).expect("digest"),
            outcome: RequestOutcome::Failed,
            error: None,
            at: later(4),
        })
        .await;
    assert!(matches!(missing, Err(PortError::InvalidRequest(_))));
    store.close().await;
}

#[tokio::test]
async fn rolled_back_claim_restores_bytes_and_the_same_id_can_be_retried() {
    let dir = temp_dir("first-submission-retry");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");
    let (transfer_a, sha_a) = stage_one_as(&store, 1, 1, b"first attachment").await;
    let (transfer_b, sha_b) = stage_one_as(&store, 1, 2, b"second attachment").await;
    let submission = seal_manifest(&store, 1, vec![transfer_a, transfer_b]).await;

    // 注入一个 sha 冲突但长度不符的正式附件行：第二个认领必然失败，整事务回滚。
    let pool = raw_write_pool(&path).await;
    sqlx::query(
        "INSERT INTO owned_attachment (attachment_id, sha256, byte_length, media_type, \
         relative_path, created_at, last_used_at) VALUES (?1, ?2, 1, 'text/plain', ?3, ?4, ?4)",
    )
    .bind(uuid(9992))
    .bind(sha_b.as_str())
    .bind(format!("attachments/{}", sha_b.as_str()))
    .bind(AT)
    .execute(&pool)
    .await
    .expect("seed conflicting attachment");
    pool.close().await;

    let mut write = submission_write(1, "with attachments");
    write.manifest = Some(acp_core::model::SubmissionManifestClaim {
        submission,
        runtime_epoch: binding(1).runtime_epoch,
        generation: ResourceGeneration::new(1),
    });
    let error = store
        .commit_first_submission(write.clone())
        .await
        .expect_err("conflicting attachment length must fail the transaction");
    assert!(matches!(
        error,
        PortError::Conflict(_) | PortError::Backend(_)
    ));
    // 回滚后第一个附件的字节必须回到封存路径（第二个尚未 rename），同页同 ID 可重试。
    assert!(
        sealed_bytes(&dir, &sha_a).is_file(),
        "回滚后字节回到封存路径"
    );
    assert!(!owned_bytes(&dir, &sha_a).exists());
    let pool = raw_pool(&path).await;
    assert_eq!(counts(&pool).await, (0, 0, 0, 0, 0, 1, 0));
    pool.close().await;

    // 去掉注入的冲突行后原样重试：同一 write、同一清单，不需要重新上传。
    let pool = raw_write_pool(&path).await;
    sqlx::query("DELETE FROM owned_attachment WHERE sha256 = ?1")
        .bind(sha_b.as_str())
        .execute(&pool)
        .await
        .expect("drop conflict");
    pool.close().await;
    let admission = store
        .commit_first_submission(write)
        .await
        .expect("retry after rollback");
    assert!(matches!(admission, FirstSubmissionAdmission::Accepted(_)));
    assert!(owned_bytes(&dir, &sha_a).is_file());
    assert!(owned_bytes(&dir, &sha_b).is_file());
    let pool = raw_pool(&path).await;
    let (attachments, links, media): (i64, i64, i64) = (
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_attachment")
            .fetch_one(&pool)
            .await
            .expect("attachments"),
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_attachment_link")
            .fetch_one(&pool)
            .await
            .expect("links"),
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_media_ref")
            .fetch_one(&pool)
            .await
            .expect("media"),
    );
    assert_eq!((attachments, links, media), (2, 2, 2));
    pool.close().await;
    store.close().await;
}

/// 回滚只能还原本清单自己搬走的字节：正式路径上的同内容字节可能属于更早已提交的会话。
#[tokio::test]
async fn rollback_keeps_bytes_committed_by_an_earlier_session() {
    let dir = temp_dir("first-submission-shared-bytes");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");
    let content = b"shared attachment bytes";
    // 两个会话为**同一份内容**准备附件：封存区按内容寻址，第二份清单复用同一路径。
    let (transfer_a, sha) = stage_one_as(&store, 1, 1, content).await;
    let (transfer_b, sha_b) = stage_one_as(&store, 3, 3, content).await;
    assert_eq!(sha_b, sha, "同一内容只有一个摘要");
    let manifest_a = seal_manifest(&store, 1, vec![transfer_a]).await;
    let manifest_b = seal_manifest(&store, 3, vec![transfer_b]).await;

    // 会话 A 先受理成功：字节 rename 到正式路径，附件行/会话引用/媒体引用已提交。
    let mut write_a = submission_write(1, "first session");
    write_a.manifest = Some(acp_core::model::SubmissionManifestClaim {
        submission: manifest_a,
        runtime_epoch: binding(1).runtime_epoch,
        generation: ResourceGeneration::new(1),
    });
    store
        .commit_first_submission(write_a)
        .await
        .expect("commit first session");
    assert!(owned_bytes(&dir, &sha).is_file());
    assert!(!sealed_bytes(&dir, &sha).exists());

    // 注入唯一键冲突，让会话 B 的受理在开始认领之后失败。
    let pool = raw_write_pool(&path).await;
    sqlx::query(
        "INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, origin_epoch, \
         version, created_at, updated_at) VALUES (?1, NULL, 'agent', 'Agent', 'idle', ?2, 1, ?3, ?3)",
    )
    .bind(session(77).as_str())
    .bind(uuid(78))
    .bind(AT)
    .execute(&pool)
    .await
    .expect("seed session");
    sqlx::query(
        "INSERT INTO owned_message (message_id, session_id, turn_id, channel, state, content_json, \
         created_at, updated_at) VALUES (?1, ?2, NULL, 'reply', 'completed', '{}', ?3, ?3)",
    )
    .bind(message(3001).as_str())
    .bind(session(77).as_str())
    .bind(AT)
    .execute(&pool)
    .await
    .expect("seed message");
    pool.close().await;

    let mut write_b = submission_write(3, "second session");
    write_b.user_message_id = message(3001);
    write_b.manifest = Some(acp_core::model::SubmissionManifestClaim {
        submission: manifest_b.clone(),
        runtime_epoch: binding(3).runtime_epoch,
        generation: ResourceGeneration::new(1),
    });
    let error = store
        .commit_first_submission(write_b)
        .await
        .expect_err("unique message id must fail the transaction");
    assert!(matches!(
        error,
        PortError::Backend(_) | PortError::Conflict(_)
    ));
    // 关键：B 的清单从未 rename 过这份字节（A 已把它放在正式路径），回滚不得把它搬回封存区。
    assert!(
        owned_bytes(&dir, &sha).is_file(),
        "已提交会话的附件字节必须原地保留"
    );
    assert!(
        !sealed_bytes(&dir, &sha).exists(),
        "回滚不得为别人的字节伪造封存副本"
    );
    let pool = raw_pool(&path).await;
    let media: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM owned_media_ref mr JOIN owned_attachment att \
         ON att.attachment_id = mr.attachment_id WHERE att.sha256 = ?1",
    )
    .bind(sha.as_str())
    .fetch_one(&pool)
    .await
    .expect("media refs");
    assert_eq!(media, 1, "A 的媒体引用仍然可解析");
    pool.close().await;

    // 冲突消除后原样重试 B 的清单：正式路径上的字节被复用（不复制、不回滚），仍然可受理。
    let mut retry = submission_write(3, "second session");
    retry.manifest = Some(acp_core::model::SubmissionManifestClaim {
        submission: manifest_b,
        runtime_epoch: binding(3).runtime_epoch,
        generation: ResourceGeneration::new(1),
    });
    let admitted = store
        .commit_first_submission(retry)
        .await
        .expect("retry after rollback");
    assert!(matches!(admitted, FirstSubmissionAdmission::Accepted(_)));
    assert!(owned_bytes(&dir, &sha).is_file());
    let pool = raw_pool(&path).await;
    let (attachments, links, media): (i64, i64, i64) = (
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_attachment")
            .fetch_one(&pool)
            .await
            .expect("attachments"),
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_attachment_link")
            .fetch_one(&pool)
            .await
            .expect("links"),
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_media_ref")
            .fetch_one(&pool)
            .await
            .expect("media"),
    );
    assert_eq!(
        (attachments, links, media),
        (1, 2, 2),
        "同一份内容只保留一行 owned_attachment"
    );
    pool.close().await;
    store.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_dispatch_fences_grant_exactly_once() {
    let dir = temp_dir("dispatch-fence-concurrent");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");
    let write = submission_write(1, "hello");
    let session_id = write.session.clone();
    let first_submission_id = write.first_submission_id.clone();
    let first_request = write.request_id.clone();
    store.commit_first_submission(write).await.expect("commit");
    let fence = fence(
        &session_id,
        &first_submission_id,
        first_request.clone(),
        None,
        "request-1",
    );
    let (left, right) = tokio::join!(
        store.begin_dispatch(fence.clone()),
        store.begin_dispatch(fence)
    );
    let left = left.expect("left dispatch");
    let right = right.expect("right dispatch");
    assert_eq!(
        [left, right]
            .iter()
            .filter(|admission| **admission == DispatchAdmission::Granted)
            .count(),
        1,
        "并发派发围栏只能有一个 Granted：{left:?} / {right:?}"
    );

    let pool = raw_pool(&path).await;
    let (stage, ever, current, agent_session): (String, i64, String, Option<String>) =
        sqlx::query_as(
            "SELECT stage, ever_dispatched, current_request_id, agent_session_id \
             FROM owned_first_submission WHERE session_id = ?1",
        )
        .bind(session_id.as_str())
        .fetch_one(&pool)
        .await
        .expect("first submission row");
    assert_eq!((stage.as_str(), ever), ("dispatch_started", 1));
    assert_eq!(current, first_request.as_str());
    assert_eq!(agent_session.as_deref(), Some("agent-session"));
    let turn_state: String =
        sqlx::query_scalar("SELECT state FROM owned_turn WHERE session_id = ?1")
            .bind(session_id.as_str())
            .fetch_one(&pool)
            .await
            .expect("turn");
    assert_eq!(turn_state, "running");
    pool.close().await;
    store.close().await;
}

/// 首条 turn 已由 `agent_host::finish_turn` + broker 路径收敛时，结算只补命令终态：不写第二个
/// `turn.*`、不改 `ended_at`、不提升会话版本，首条投影仍然收敛。
#[tokio::test]
async fn settlement_does_not_repeat_an_already_terminal_turn() {
    let dir = temp_dir("first-submission-broker-terminal");
    let store = open(&dir).await;
    let path = dir.join("acp-remote.sqlite3");
    let write = submission_write(1, "hello");
    let session_id = write.session.clone();
    let first_submission_id = write.first_submission_id.clone();
    let first_request = write.request_id.clone();
    let first_turn = write.turn.clone();
    store.commit_first_submission(write).await.expect("commit");
    assert_eq!(
        store
            .begin_dispatch(fence(
                &session_id,
                &first_submission_id,
                first_request.clone(),
                None,
                "request-1",
            ))
            .await
            .expect("dispatch"),
        DispatchAdmission::Granted
    );

    // 模拟 broker 的终态提交（`agent_host::finish_turn` + `commit_chunk`）：turn 已终态、会话已
    // idle 且版本 +1，并已有那条 `turn.completed` 事件。
    let broker_event = uuid(7001);
    let broker_payload = serde_json::json!({
        "turnId": first_turn.as_str(),
        "state": "completed",
    })
    .to_string();
    let ended = later(1);
    let pool = raw_write_pool(&path).await;
    let origin_epoch: String =
        sqlx::query_scalar("SELECT origin_epoch FROM owned_session WHERE session_id = ?1")
            .bind(session_id.as_str())
            .fetch_one(&pool)
            .await
            .expect("origin epoch");
    sqlx::query("UPDATE owned_turn SET state = 'completed', ended_at = ?1 WHERE turn_id = ?2")
        .bind(ended.as_str())
        .bind(first_turn.as_str())
        .execute(&pool)
        .await
        .expect("turn terminal");
    sqlx::query(
        "UPDATE owned_session SET state = 'idle', version = version + 1, updated_at = ?1 \
         WHERE session_id = ?2",
    )
    .bind(ended.as_str())
    .bind(session_id.as_str())
    .execute(&pool)
    .await
    .expect("session idle");
    sqlx::query(
        "INSERT INTO owned_event (global_sequence, session_id, session_sequence, origin_epoch, \
         origin_sequence, event_id, turn_id, event_type, kind, policy, origin_kind, payload_json, \
         payload_digest, created_at) \
         VALUES (2, ?1, 2, ?2, 2, ?3, ?4, 'turn.completed', 'state', 'durable', 'agent', ?5, ?6, ?7)",
    )
    .bind(session_id.as_str())
    .bind(&origin_epoch)
    .bind(&broker_event)
    .bind(first_turn.as_str())
    .bind(&broker_payload)
    .bind(digest_text("broker turn.completed"))
    .bind(ended.as_str())
    .execute(&pool)
    .await
    .expect("broker event");
    let version_before: i64 =
        sqlx::query_scalar("SELECT version FROM owned_session WHERE session_id = ?1")
            .bind(session_id.as_str())
            .fetch_one(&pool)
            .await
            .expect("version");
    assert_eq!(version_before, 2);
    pool.close().await;

    // 结算：命令终态补上，但 turn/会话事实不得被第二次推进（也不写 `session.updated`）。
    let settled = store
        .settle_first_submission(FirstSubmissionSettlement {
            session: session_id.clone(),
            actor: Actor::Device {
                device: device(900),
            },
            request_id: first_request.clone(),
            request_fingerprint: Digest::new(&digest_text("request-1")).expect("digest"),
            outcome: RequestOutcome::Completed,
            error: None,
            at: later(2),
        })
        .await
        .expect("settle");
    assert_eq!(settled.stage, FirstSubmissionStage::Terminal);
    assert_eq!(
        settled.terminal.as_ref().map(|terminal| terminal.outcome),
        Some(RequestOutcome::Completed)
    );

    let pool = raw_pool(&path).await;
    let (turn_state, turn_ended): (String, Option<String>) =
        sqlx::query_as("SELECT state, ended_at FROM owned_turn WHERE turn_id = ?1")
            .bind(first_turn.as_str())
            .fetch_one(&pool)
            .await
            .expect("turn");
    assert_eq!(turn_state, "completed");
    assert_eq!(
        turn_ended.as_deref(),
        Some(ended.as_str()),
        "结算不得改写已终态 turn 的 ended_at"
    );
    let (version, state): (i64, String) =
        sqlx::query_as("SELECT version, state FROM owned_session WHERE session_id = ?1")
            .bind(session_id.as_str())
            .fetch_one(&pool)
            .await
            .expect("session");
    assert_eq!(
        (version, state.as_str()),
        (2, "idle"),
        "结算不得再次提升会话版本"
    );
    let turn_events: Vec<(String, String)> = sqlx::query_as(
        "SELECT event_id, event_type FROM owned_event WHERE turn_id = ?1 \
         AND event_type IN ('turn.completed','turn.failed') ORDER BY session_sequence ASC",
    )
    .bind(first_turn.as_str())
    .fetch_all(&pool)
    .await
    .expect("turn events");
    assert_eq!(
        turn_events,
        vec![(broker_event, "turn.completed".to_owned())],
        "同一 turn 只能有一个终态事件"
    );
    let (status, result_json): (String, Option<String>) =
        sqlx::query_as("SELECT status, result_json FROM owned_command WHERE request_id = ?1")
            .bind(first_request.as_str())
            .fetch_one(&pool)
            .await
            .expect("command");
    assert_eq!(status, "completed");
    assert!(result_json.is_some(), "命令终态仍补齐 §11.9 的六字段投影");
    let order: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM owned_event WHERE session_id = ?1 ORDER BY session_sequence ASC",
    )
    .bind(session_id.as_str())
    .fetch_all(&pool)
    .await
    .expect("events");
    assert_eq!(
        order,
        vec!["user.message.delta", "turn.completed", "command.completed"]
    );
    pool.close().await;
    store.close().await;
}
