//! §6 第 14/15 条、§7.6：消息通道、部分正文、媒体引用、压缩后的正文恢复与保留期回收。

mod support;

use acp_core::model::{
    AgentId, AgentRef, AttachmentId, AttachmentSubject, DeviceId, Digest, EventKind, EventOrigin,
    EventPayload, EventType, GlobalCursor, MediaAvailability, MediaKind, MessageChannel, MessageId,
    MessageState, OriginEpoch, PartialReason, PendingEvent, PortError, ServerEpoch, SessionId,
    StoredPolicy, Timestamp, TurnId, ViewJson,
};
use acp_core::ports::{
    AttachmentTransferStore, HistoryInclude, HistoryQuery, NewSession, OwnedCommit, ReplayLimit,
    RetentionPolicy, SessionStore, StateChange, TransferBegin, TransferChunk, TransferSeal,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
const DEVICE: &str = "55555555-5555-4555-8555-555555555555";

fn at(date: &str) -> Timestamp {
    Timestamp::new(&format!("{date}T00:00:00.000Z")).expect("timestamp")
}

fn epoch() -> OriginEpoch {
    OriginEpoch::new(EPOCH).expect("epoch")
}

fn turn(n: u8) -> TurnId {
    TurnId::new(&format!("33333333-3333-4333-8333-{n:012}")).expect("turn")
}

fn message(n: u64) -> MessageId {
    MessageId::new(&format!("40000000-0000-4000-8000-{n:012}")).expect("message")
}

fn attachment(n: u64) -> AttachmentId {
    AttachmentId::new(&format!("50000000-0000-4000-8000-{n:012}")).expect("attachment")
}

async fn open_store(dir: &std::path::Path) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at("2026-09-18"))
        .await
        .expect("open")
}

async fn create_session(store: &SqliteStore) -> SessionId {
    store
        .commit(OwnedCommit {
            session: None,
            at: at("2026-09-18"),
            expected_version: None,
            state: Some(StateChange::Create(NewSession {
                title: Some("messages".to_owned()),
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
            origin_epoch: Some(epoch()),
            input: None,
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id")
}

fn event(
    kind: EventKind,
    event_type: &str,
    view: &str,
    turn: Option<TurnId>,
    _at: &Timestamp,
) -> PendingEvent {
    PendingEvent::new(
        kind,
        EventType::new(event_type).expect("event type"),
        StoredPolicy::Durable,
        EventPayload::new(ViewJson::new(view).expect("view"), None),
        EventOrigin::Agent,
        turn,
        None,
    )
}

fn commit(
    session: &SessionId,
    at: &Timestamp,
    events: Vec<PendingEvent>,
    compacted: Vec<GlobalCursor>,
) -> OwnedCommit {
    // `owned_event.turn_id` 是 `owned_turn` 的外键：为测试里出现的 turn 建行（真实路径由 core 的
    // `TurnChange::Create` 完成）。
    let mut turns: Vec<acp_core::ports::TurnChange> = Vec::new();
    let mut seen = Vec::new();
    for event in &events {
        if let Some(turn) = &event.turn
            && !seen.iter().any(|existing: &TurnId| existing == turn)
        {
            seen.push(turn.clone());
            turns.push(acp_core::ports::TurnChange::Create(
                acp_core::ports::NewTurn {
                    turn: turn.clone(),
                    state: acp_core::model::TurnState::Running,
                    causation: None,
                    started_at: Some(at.clone()),
                },
            ));
        }
    }
    OwnedCommit {
        session: Some(session.clone()),
        at: at.clone(),
        expected_version: None,
        state: None,
        turns,
        events,
        interactions: Vec::new(),
        compacted,
        idempotency: None,
        command_terminal: None,
        origin_epoch: None,
        input: None,
    }
}

async fn read_messages(
    store: &SqliteStore,
    session: &SessionId,
    include_history: bool,
) -> acp_core::ports::HistoryPage {
    let view = store.read_view().await.expect("view");
    view.read_session(HistoryQuery {
        session: session.clone(),
        include: HistoryInclude {
            messages: true,
            turns: false,
            pending_interactions: false,
            config_options: false,
            capabilities: false,
            history: include_history,
        },
        after: None,
        before_history: None,
        before_messages: None,
        messages_limit: ReplayLimit::new(500),
        limit: ReplayLimit::new(500),
    })
    .await
    .expect("read session")
}

fn reply_delta(turn: &TurnId, message: &MessageId, text: &str) -> PendingEvent {
    event(
        EventKind::Delta,
        "agent.message.delta",
        &format!(
            r#"{{"messageId":"{}","turnId":"{}","deltaIndex":"0","text":"{text}"}}"#,
            message.as_str(),
            turn.as_str()
        ),
        Some(turn.clone()),
        &at("2026-09-18"),
    )
}

fn thought_delta(turn: &TurnId, message: &MessageId, text: &str) -> PendingEvent {
    event(
        EventKind::Delta,
        "agent.thought.delta",
        &format!(
            r#"{{"messageId":"{}","turnId":"{}","deltaIndex":"0","text":"{text}"}}"#,
            message.as_str(),
            turn.as_str()
        ),
        Some(turn.clone()),
        &at("2026-09-18"),
    )
}

fn completed(turn: &TurnId, message: &MessageId, content: &str, hour: &str) -> PendingEvent {
    event(
        EventKind::FinalMessage,
        "agent.message.completed",
        &format!(
            r#"{{"messageId":"{}","turnId":"{}","content":{content}}}"#,
            message.as_str(),
            turn.as_str()
        ),
        Some(turn.clone()),
        &at(hour),
    )
}

fn terminal(kind: &str, turn: &TurnId, hour: &str) -> PendingEvent {
    event(
        EventKind::State,
        kind,
        &format!(
            r#"{{"turnId":"{}","state":"{}"}}"#,
            turn.as_str(),
            match kind {
                "turn.completed" => "completed",
                "turn.cancelled" => "cancelled",
                _ => "failed",
            }
        ),
        Some(turn.clone()),
        &at(hour),
    )
}

/// 回复与思考分别保存；停止/失败/缺失完成事件都保留已收到正文并标记原因。
#[tokio::test]
async fn reply_and_thought_channels_keep_partial_bodies() {
    let dir = temp_dir("messages-channels");
    let store = open_store(&dir).await;
    let session = create_session(&store).await;

    let first_turn = turn(1);
    let reply = message(1);
    let thought = message(2);
    store
        .commit(commit(
            &session,
            &at("2026-09-18"),
            vec![
                reply_delta(&first_turn, &reply, "Hel"),
                reply_delta(&first_turn, &reply, "lo"),
                thought_delta(&first_turn, &thought, "thinking"),
                completed(
                    &first_turn,
                    &reply,
                    r#"[{"type":"text","text":"Hello"}]"#,
                    "2026-09-18",
                ),
                terminal("turn.completed", &first_turn, "2026-09-18"),
            ],
            Vec::new(),
        ))
        .await
        .expect("first turn");

    let page = read_messages(&store, &session, false).await;
    assert_eq!(page.messages.len(), 2);
    let reply_record = page
        .messages
        .iter()
        .find(|record| record.message == reply)
        .expect("reply");
    assert_eq!(reply_record.channel, MessageChannel::Reply);
    assert_eq!(reply_record.result.state, MessageState::Completed);
    assert_eq!(reply_record.result.reason, None);
    assert_eq!(
        reply_record.content.as_str(),
        r#"[{"text":"Hello","type":"text"}]"#,
        "completed 事件以权威正文覆盖 delta 折叠结果"
    );
    let thought_record = page
        .messages
        .iter()
        .find(|record| record.message == thought)
        .expect("thought");
    assert_eq!(thought_record.channel, MessageChannel::Thought);
    assert_eq!(thought_record.result.state, MessageState::Completed);
    assert_eq!(
        thought_record.content.as_str(),
        r#"[{"text":"thinking","type":"text"}]"#
    );

    // 停止：两类正文都保留，原因可区分。
    let cancelled_turn = turn(2);
    let cancelled_reply = message(3);
    let cancelled_thought = message(4);
    store
        .commit(commit(
            &session,
            &at("2026-09-18"),
            vec![
                reply_delta(&cancelled_turn, &cancelled_reply, "partial"),
                thought_delta(&cancelled_turn, &cancelled_thought, "still thinking"),
                terminal("turn.cancelled", &cancelled_turn, "2026-09-18"),
            ],
            Vec::new(),
        ))
        .await
        .expect("cancelled turn");
    let page = read_messages(&store, &session, false).await;
    for id in [&cancelled_reply, &cancelled_thought] {
        let record = page
            .messages
            .iter()
            .find(|record| &record.message == id)
            .expect("record");
        assert_eq!(record.result.state, MessageState::Cancelled);
        assert_eq!(record.result.reason, Some(PartialReason::TurnCancelled));
    }

    // 失败。
    let failed_turn = turn(3);
    let failed_reply = message(5);
    store
        .commit(commit(
            &session,
            &at("2026-09-18"),
            vec![
                reply_delta(&failed_turn, &failed_reply, "boom"),
                terminal("turn.failed", &failed_turn, "2026-09-18"),
            ],
            Vec::new(),
        ))
        .await
        .expect("failed turn");
    let page = read_messages(&store, &session, false).await;
    let record = page
        .messages
        .iter()
        .find(|record| record.message == failed_reply)
        .expect("failed reply");
    assert_eq!(record.result.state, MessageState::Failed);
    assert_eq!(record.result.reason, Some(PartialReason::AgentFailed));

    // 正常终态但缺 `agent.message.completed`：不得伪装成完整成功。
    let incomplete_turn = turn(4);
    let incomplete = message(6);
    store
        .commit(commit(
            &session,
            &at("2026-09-18"),
            vec![
                reply_delta(&incomplete_turn, &incomplete, "cut"),
                terminal("turn.completed", &incomplete_turn, "2026-09-18"),
            ],
            Vec::new(),
        ))
        .await
        .expect("completed turn without completion event");
    let page = read_messages(&store, &session, false).await;
    let record = page
        .messages
        .iter()
        .find(|record| record.message == incomplete)
        .expect("incomplete reply");
    assert_eq!(record.result.state, MessageState::Failed);
    assert_eq!(record.result.reason, Some(PartialReason::AgentFailed));
    assert_eq!(record.content.as_str(), r#"[{"text":"cut","type":"text"}]"#);

    store.close().await;
}

fn digest_of(data: &[u8]) -> Digest {
    Digest::new(&digest_text(&String::from_utf8_lossy(data))).expect("digest")
}

async fn seal_bytes(
    store: &SqliteStore,
    session: &SessionId,
    id: u64,
    data: &[u8],
    at: &Timestamp,
) -> AttachmentId {
    let transfer = attachment(id);
    store
        .begin_transfer(TransferBegin {
            transfer: transfer.clone(),
            session: session.clone(),
            subject: AttachmentSubject::Device(DeviceId::new(DEVICE).expect("device")),
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
            at: at.clone(),
        })
        .await
        .expect("seal")
        .attachment
}

/// 媒体引用只存引用与类型；未知附件使整条消息提交失败（不写半条正文）。
#[tokio::test]
async fn media_references_are_typed_and_validated() {
    let dir = temp_dir("messages-media");
    let store = open_store(&dir).await;
    let session = create_session(&store).await;
    let image = seal_bytes(&store, &session, 10, b"png", &at("2026-09-18")).await;
    let audio = seal_bytes(&store, &session, 11, b"wav", &at("2026-09-18")).await;
    let file = seal_bytes(&store, &session, 12, b"txt", &at("2026-09-18")).await;
    let message_id = message(20);
    let turn_id = turn(5);
    let content = format!(
        r#"[{{"type":"text","text":"media"}},
            {{"type":"image_ref","attachmentId":"{image}","mimeType":"image/png","byteLength":"3","displayState":"available"}},
            {{"type":"audio_ref","attachmentId":"{audio}","mimeType":"audio/wav","byteLength":"3","displayState":"available"}},
            {{"type":"file_ref","attachmentId":"{file}","mimeType":"text/plain","byteLength":"3","displayState":"available"}}]"#,
        image = image.as_str(),
        audio = audio.as_str(),
        file = file.as_str(),
    );
    store
        .commit(commit(
            &session,
            &at("2026-09-18"),
            vec![
                completed(&turn_id, &message_id, &content, "2026-09-18"),
                terminal("turn.completed", &turn_id, "2026-09-18"),
            ],
            Vec::new(),
        ))
        .await
        .expect("media message");

    let page = read_messages(&store, &session, false).await;
    let record = page
        .messages
        .iter()
        .find(|record| record.message == message_id)
        .expect("message");
    assert_eq!(record.media.len(), 3);
    assert_eq!(record.media[0].kind(), MediaKind::Image);
    assert_eq!(record.media[1].kind(), MediaKind::Audio);
    assert_eq!(record.media[2].kind(), MediaKind::File);
    assert_eq!(record.media[0].availability(), MediaAvailability::Available);
    // 媒体引用本身即会话归属证据：无需 `link` 也能按会话读取。
    let chunk = store
        .read_chunk(&acp_core::ports::AttachmentRead {
            attachment: image,
            session: session.clone(),
            offset: 0,
            max_length: 8,
        })
        .await
        .expect("read media");
    assert_eq!(chunk.data, b"png");

    // 未知附件 → 整事务回滚，不留消息/媒体行。
    let unknown_message = message(21);
    let unknown = attachment(99);
    let error = store
        .commit(commit(
            &session,
            &at("2026-09-18"),
            vec![completed(
                &turn(9),
                &unknown_message,
                &format!(
                    r#"[{{"type":"image_ref","attachmentId":"{}","mimeType":"image/png","byteLength":"1","displayState":"available"}}]"#,
                    unknown.as_str()
                ),
                "2026-09-18",
            )],
            Vec::new(),
        ))
        .await
        .expect_err("unknown attachment");
    assert!(matches!(error, PortError::InvalidRequest(_)));
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(
            &pool,
            &format!(
                "SELECT COUNT(*) FROM owned_message WHERE message_id = '{}'",
                unknown_message.as_str()
            )
        )
        .await,
        0
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_media_ref").await,
        3,
        "失败事务不得改掉既有消息的媒体引用"
    );
    pool.close().await;

    store.close().await;
}

/// 压缩只替换 delta 事件；正文保存在 `owned_message`，重开与压缩后仍可恢复。
#[tokio::test]
async fn compaction_keeps_the_authoritative_message_body() {
    let dir = temp_dir("messages-compaction");
    let store = open_store(&dir).await;
    let session = create_session(&store).await;
    let turn_id = turn(6);
    let message_id = message(30);
    let written = store
        .commit(commit(
            &session,
            &at("2026-09-18"),
            vec![reply_delta(&turn_id, &message_id, "body")],
            Vec::new(),
        ))
        .await
        .expect("delta");
    let replaced = written.appended[0].clone();
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let server_epoch = ServerEpoch::new(
        &sqlx::query_scalar::<_, String>("SELECT value FROM meta WHERE key = 'server_epoch'")
            .fetch_one(&pool)
            .await
            .expect("epoch"),
    )
    .expect("server epoch");
    pool.close().await;

    store
        .commit(commit(
            &session,
            &at("2026-09-18"),
            vec![event(
                EventKind::Summary,
                "turn.delta_compacted",
                &format!(r#"{{"turnId":"{}","deltaCount":1}}"#, turn_id.as_str()),
                None,
                &at("2026-09-18"),
            )],
            vec![GlobalCursor::new(server_epoch, replaced.global_sequence)],
        ))
        .await
        .expect("compaction");

    // 压缩后事件页不再出现 delta，但正文仍在消息面。
    let page = read_messages(&store, &session, false).await;
    assert!(
        page.events
            .iter()
            .all(|event| event.event.event_type.as_str() != "agent.message.delta"),
        "被压缩的 delta 不再出现在事件页"
    );
    let record = page
        .messages
        .iter()
        .find(|record| record.message == message_id)
        .expect("message");
    assert_eq!(
        record.content.as_str(),
        r#"[{"text":"body","type":"text"}]"#
    );

    // 重开存储后正文仍可读（不依赖进程内状态）。
    store.close().await;
    let reopened = open_store(&dir).await;
    let page = read_messages(&reopened, &session, false).await;
    let record = page
        .messages
        .iter()
        .find(|record| record.message == message_id)
        .expect("message after reopen");
    assert_eq!(
        record.content.as_str(),
        r#"[{"text":"body","type":"text"}]"#
    );
    reopened.close().await;
}

/// 正文随保留期清理；共享媒体引用在其最后一条消息消失前保留附件，随后按引用回收。
#[tokio::test]
async fn retention_and_gc_respect_shared_message_references() {
    let dir = temp_dir("messages-gc");
    let store = open_store(&dir).await;
    let session = create_session(&store).await;
    let shared = seal_bytes(&store, &session, 40, b"shared", &at("2026-09-18")).await;
    let turn_id = turn(7);
    let first = message(40);
    let second = message(41);
    let content = format!(
        r#"[{{"type":"file_ref","attachmentId":"{}","mimeType":"text/plain","byteLength":"6","displayState":"available"}}]"#,
        shared.as_str()
    );
    store
        .commit(commit(
            &session,
            &at("2026-09-18"),
            vec![completed(&turn_id, &first, &content, "2026-09-18")],
            Vec::new(),
        ))
        .await
        .expect("first message");
    store
        .commit(commit(
            &session,
            &at("2027-01-01"),
            vec![completed(&turn(8), &second, &content, "2027-01-01")],
            Vec::new(),
        ))
        .await
        .expect("second message");

    let policy = RetentionPolicy {
        transcript_retention_days: 90,
        sync_event_retention_days: 7,
        audit_retention_days: 365,
        max_total_size_bytes: 2 * 1024 * 1024 * 1024,
        max_session_size_bytes: 100 * 1024 * 1024,
        persist_deltas: false,
    };
    // 2026-12-27（+100 天）：第一条消息已超 90 天、第二条仍在。
    let report = store
        .prune(policy, at("2026-12-27"))
        .await
        .expect("prune first");
    assert_eq!(report.removed_messages, 1);
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_message").await,
        1
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_media_ref").await,
        1,
        "第二条消息的引用随消息保留"
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        1,
        "仍被消息引用的附件不得回收"
    );
    pool.close().await;

    // 2027-04-06（+200 天）：第二条也超期；正文与引用删除后，附件在事务外按引用回收。
    let report = store
        .prune(policy, at("2027-04-06"))
        .await
        .expect("prune second");
    assert_eq!(report.removed_messages, 1);
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_message").await,
        0
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        0
    );
    pool.close().await;
    assert!(
        !dir.join("attachments").join(digest_text("shared")).exists(),
        "行删除提交后文件才被回收"
    );

    store.close().await;
}

/// `conversation-reliability`：Agent 不回显的第二条输入，刷新/重新打开后仍完整可见。
///
/// 正文只来自受理事务写下的 `owned_message`（`channel='user'`），不依赖任何回显事件。
#[tokio::test]
async fn prompt_input_without_echo_remains_readable() {
    let dir = temp_dir("messages-prompt-no-echo");
    let store = open_store(&dir).await;
    let session = create_session(&store).await;
    let turn = acp_core::model::TurnId::new("33333333-3333-4333-8333-000000000042").expect("turn");
    let message =
        acp_core::model::MessageId::new("40000000-0000-4000-8000-000000000042").expect("message");
    let text = "第二条没有回显的输入";
    let content = acp_core::model::FirstSubmissionInput::new(vec![
        acp_core::model::PromptContentBlock::from_json_text(
            &serde_json::json!({"type": "text", "text": text}).to_string(),
        )
        .expect("block"),
    ])
    .expect("input");
    let request =
        acp_core::model::RequestId::new("66666666-6666-4666-8666-000000000042").expect("request");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at("2026-09-19"),
            expected_version: None,
            state: None,
            turns: vec![acp_core::ports::TurnChange::Create(
                acp_core::ports::NewTurn {
                    turn: turn.clone(),
                    state: acp_core::model::TurnState::Running,
                    causation: Some(request.clone()),
                    started_at: None,
                },
            )],
            events: vec![event(
                EventKind::State,
                "turn.queued",
                r#"{"state":"queued"}"#,
                Some(turn.clone()),
                &at("2026-09-19"),
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(acp_core::ports::IdempotencyRecord {
                actor: acp_core::model::Actor::Device {
                    device: DeviceId::new(DEVICE).expect("device"),
                },
                request: request.clone(),
                command: "session.prompt".to_owned(),
                kind: acp_core::model::CommandKind::Mutation,
                session: Some(session.clone()),
                expected_version: None,
                request_fingerprint: Digest::new(&digest_text("no-echo-request")).expect("digest"),
                accepted_at: at("2026-09-19"),
            }),
            command_terminal: None,
            origin_epoch: Some(epoch()),
            input: Some(acp_core::model::UserInputWrite {
                message: message.clone(),
                session: session.clone(),
                turn: turn.clone(),
                request,
                content: content.clone(),
                at: at("2026-09-19"),
            }),
        })
        .await
        .expect("commit input");
    store.close().await;

    // 进程重启后重新打开：没有 Agent 回显也能完整读回。
    let reopened = open_store(&dir).await;
    let page = read_messages(&reopened, &session, false).await;
    assert_eq!(page.messages.len(), 1, "恰好一条用户消息");
    let stored = &page.messages[0];
    assert_eq!(stored.message, message);
    assert_eq!(stored.channel, MessageChannel::User);
    assert_eq!(stored.turn, Some(turn));
    assert_eq!(stored.content.as_str(), content.json().as_str());
    assert!(stored.media.is_empty());
    reopened.close().await;
}
