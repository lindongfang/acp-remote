//! v0.4 §6 第 15/16 条的 storage 侧判据（§9 判据 19 的存储部分、判据 20）。

mod support;

use acp_core::model::{
    AgentId, AgentRef, CommandKind, Digest, EventKind, EventType, GlobalCursor, OriginEpoch,
    PendingEvent, PortError, RequestId, Sequence, ServerEpoch, SessionId, StoredPolicy, Timestamp,
    ViewJson,
};
use acp_core::ports::{
    IdempotencyRecord, NewSession, OwnedCommit, ReplayLimit, SessionStore, StateChange,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";

fn at(hour: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{hour:02}:00:00.000Z")).expect("timestamp")
}

fn epoch() -> OriginEpoch {
    OriginEpoch::new(EPOCH).expect("epoch")
}

fn pending(kind: EventKind, event_type: &str, view: &str) -> PendingEvent {
    PendingEvent::new(
        kind,
        EventType::new(event_type).expect("event type"),
        StoredPolicy::Durable,
        acp_core::model::EventPayload::new(ViewJson::new(view).expect("view"), None),
        acp_core::model::EventOrigin::Agent,
        None,
        None,
    )
}

async fn store(dir: &std::path::Path) -> SqliteStore {
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
                title: Some("compaction".to_owned()),
                agent: AgentRef::try_new(
                    AgentId::new("probe-agent").expect("agent"),
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

fn commit(session: &SessionId, hour: u32) -> OwnedCommit {
    OwnedCommit {
        session: Some(session.clone()),
        at: at(hour),
        expected_version: None,
        state: None,
        turns: Vec::new(),
        events: Vec::new(),
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: None,
        command_terminal: None,
        origin_epoch: Some(epoch()),
        input: None,
    }
}

/// §6 第 15 条 / §9 判据 19：`compacted` 指向的 delta 行被写成指向 summary 事件的 `global_sequence`，
/// 且 `replay` 里不再出现这些行（由 summary 事件替代）。
#[tokio::test]
async fn compaction_points_the_deltas_at_the_summary_event() {
    let dir = temp_dir("compaction-ok");
    let store = store(&dir).await;
    let session = create_session(&store).await;

    let mut deltas = commit(&session, 1);
    for index in 0..3 {
        deltas.events.push(pending(
            EventKind::Delta,
            "agent.message",
            &format!(r#"{{"n":{index}}}"#),
        ));
    }
    let written = store.commit(deltas).await.expect("deltas");
    assert_eq!(written.appended.len(), 3);
    let replaced = [written.appended[0].clone(), written.appended[1].clone()];

    let mut compaction = commit(&session, 2);
    compaction.events.push(pending(
        EventKind::Summary,
        "turn.delta_compacted",
        r#"{"deltaCount":2,"turnId":"33333333-3333-4333-8333-333333333333"}"#,
    ));
    let server_epoch = ServerEpoch::new(&store_epoch(&dir).await).expect("epoch");
    compaction.compacted = replaced
        .iter()
        .map(|event| GlobalCursor::new(server_epoch.clone(), event.global_sequence))
        .collect();
    let summary = store.commit(compaction).await.expect("compaction");
    let summary_sequence = summary.appended[0].global_sequence;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    for event in &replaced {
        let compacted_into: Option<i64> =
            sqlx::query_scalar("SELECT compacted_into FROM owned_event WHERE global_sequence = ?1")
                .bind(i64::try_from(event.global_sequence.get()).expect("seq"))
                .fetch_one(&pool)
                .await
                .expect("row");
        assert_eq!(
            compacted_into,
            Some(i64::try_from(summary_sequence.get()).expect("seq")),
            "the replaced delta must point at the summary event"
        );
    }
    pool.close().await;

    // 重放里不再出现被压缩的两行；未压缩的那一行仍在。
    let view = store.read_view().await.expect("view");
    let batch = view
        .replay(None, ReplayLimit::default())
        .await
        .expect("replay");
    let sequences: Vec<u64> = batch
        .events
        .iter()
        .map(|event| event.global_sequence.get())
        .collect();
    assert_eq!(
        sequences,
        vec![
            written.appended[2].global_sequence.get(),
            summary_sequence.get()
        ],
        "compacted deltas are replaced by the summary event"
    );
    drop(view);
    store.close().await;
}

async fn store_epoch(dir: &std::path::Path) -> String {
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let epoch: String = sqlx::query_scalar("SELECT value FROM meta WHERE key = 'server_epoch'")
        .fetch_one(&pool)
        .await
        .expect("server epoch");
    pool.close().await;
    epoch
}

/// §6 第 15 条负例：四种非法 `compacted` 都必须整事务 `InvalidRequest` 且**零写入**。
#[tokio::test]
async fn invalid_compaction_is_rejected_without_writing() {
    let dir = temp_dir("compaction-invalid");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    let other = create_session(&store).await;

    let mut deltas = commit(&session, 1);
    deltas
        .events
        .push(pending(EventKind::Delta, "agent.message", r#"{"n":1}"#));
    deltas
        .events
        .push(pending(EventKind::FinalMessage, "agent.message", "{}"));
    let written = store.commit(deltas).await.expect("deltas");
    let delta = written.appended[0].global_sequence;
    let non_delta = written.appended[1].global_sequence;
    let server_epoch = ServerEpoch::new(&store_epoch(&dir).await).expect("epoch");

    let mut other_commit = commit(&other, 1);
    other_commit
        .events
        .push(pending(EventKind::Delta, "agent.message", r#"{"other":1}"#));
    let other_written = store.commit(other_commit).await.expect("other delta");

    let cases: [(&str, OwnedCommit); 4] = [
        // 1) 非 delta 行
        ("non-delta", {
            let mut c = commit(&session, 2);
            c.events
                .push(pending(EventKind::Summary, "turn.delta_compacted", "{}"));
            c.compacted = vec![GlobalCursor::new(server_epoch.clone(), non_delta)];
            c
        }),
        // 2) 不存在的 cursor
        ("missing", {
            let mut c = commit(&session, 2);
            c.events
                .push(pending(EventKind::Summary, "turn.delta_compacted", "{}"));
            c.compacted = vec![GlobalCursor::new(
                server_epoch.clone(),
                Sequence::new(999).expect("sequence"),
            )];
            c
        }),
        // 3) 跨会话 cursor
        ("cross-session", {
            let mut c = commit(&session, 2);
            c.events
                .push(pending(EventKind::Summary, "turn.delta_compacted", "{}"));
            c.compacted = vec![GlobalCursor::new(
                server_epoch.clone(),
                other_written.appended[0].global_sequence,
            )];
            c
        }),
        // 4) 提交里没有 summary 事件
        ("no-summary", {
            let mut c = commit(&session, 2);
            c.compacted = vec![GlobalCursor::new(server_epoch.clone(), delta)];
            c
        }),
    ];

    let path = dir.join("acp-remote.sqlite3");
    for (label, case) in cases {
        let pool = raw_pool(&path).await;
        let before = table_snapshot(&pool, "owned_event").await;
        pool.close().await;
        let error = store
            .commit(case)
            .await
            .expect_err(&format!("`{label}` must be rejected"));
        assert!(
            matches!(error, PortError::InvalidRequest(_)),
            "`{label}` must be InvalidRequest, got {error}"
        );
        let pool = raw_pool(&path).await;
        assert_eq!(
            table_snapshot(&pool, "owned_event").await,
            before,
            "`{label}` must leave zero writes behind"
        );
        pool.close().await;
    }
    store.close().await;
}

/// §6 第 16 条：`unsettled_commands` 只返回 `accepted` 且没有终态事件的 mutation 行，按 `accepted_at`
/// 升序，`limit` 生效。
#[tokio::test]
async fn unsettled_commands_lists_only_accepted_mutations() {
    use acp_core::model::{CommandStatus, CommandTerminalRecord};

    let dir = temp_dir("unsettled");
    let store = store(&dir).await;
    let session = create_session(&store).await;

    let record = |request: &str, at_hour: u32| IdempotencyRecord {
        actor: acp_core::model::Actor::LocalCli,
        request: RequestId::new(request).expect("request"),
        command: "session.prompt".to_owned(),
        kind: CommandKind::Mutation,
        session: Some(session.clone()),
        expected_version: None,
        request_fingerprint: Digest::new(&support::digest_text(request)).expect("digest"),
        accepted_at: at(at_hour),
    };
    let later_request = "11111111-1111-4111-8111-111111111111";
    let earlier_request = "22222222-2222-4222-8222-222222222222";
    let done_request = "33333333-3333-4333-8333-333333333333";

    // 两条 accepted：`accepted_at` 与提交顺序相反，用来证明排序键是 `accepted_at`。
    let mut later = commit(&session, 2);
    later.idempotency = Some(record(later_request, 5));
    store.commit(later).await.expect("accepted (late)");
    let mut earlier = commit(&session, 3);
    earlier.idempotency = Some(record(earlier_request, 1));
    store.commit(earlier).await.expect("accepted (early)");

    // 对照组：一条被推进到终态的 mutation 行（`completed` 且有终态事件）不得出现在结果里。
    let mut done = commit(&session, 4);
    done.idempotency = Some(record(done_request, 3));
    store.commit(done).await.expect("accepted (to be done)");
    let mut terminal = commit(&session, 5);
    terminal.events.push(pending(
        EventKind::Structured,
        "command.completed",
        &format!(r#"{{"requestId":"{done_request}"}}"#),
    ));
    terminal.events[0].causation = Some(RequestId::new(done_request).expect("request"));
    terminal.command_terminal = Some(
        CommandTerminalRecord::try_new(CommandStatus::Completed, Some(at(5)), None, None, None)
            .expect("terminal"),
    );
    store
        .commit(terminal)
        .await
        .expect("complete the control row");

    // 终态行必须被排除（否则恢复流程会重复确认已经完成的命令）。
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_command WHERE status = 'completed'"
        )
        .await,
        1
    );
    pool.close().await;

    let unsettled = store
        .unsettled_commands(ReplayLimit::new(10))
        .await
        .expect("unsettled");
    let requests: Vec<&str> = unsettled.iter().map(|row| row.request().as_str()).collect();
    assert_eq!(
        requests,
        vec![earlier_request, later_request],
        "only accepted mutations, ordered by accepted_at (not by insertion order)"
    );
    assert!(
        unsettled
            .iter()
            .all(|row| row.status() == CommandStatus::Accepted)
    );

    // limit 生效。
    let first = store
        .unsettled_commands(ReplayLimit::new(1))
        .await
        .expect("limited");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].request().as_str(), earlier_request);
    store.close().await;
}

/// `conversation-reliability`：超过单条事件文本上限的输入，在 `user.message.delta` 被压缩后
/// 仍从 `owned_message` 完整读回；事件文本按上限截断。
#[tokio::test]
async fn long_prompt_input_survives_delta_compaction() {
    use acp_core::model::{FirstSubmissionInput, PromptContentBlock, TurnState};

    let dir = temp_dir("compaction-long-input");
    let store = store(&dir).await;
    let session = create_session(&store).await;

    let text = "长".repeat(300_000);
    let content = FirstSubmissionInput::new(vec![
        PromptContentBlock::from_json_text(
            &serde_json::json!({"type": "text", "text": text}).to_string(),
        )
        .expect("block"),
    ])
    .expect("input");
    let projected = acp_core::model::prompt_message_text(&content);
    assert_eq!(
        projected.chars().count(),
        acp_core::model::PROMPT_MESSAGE_TEXT_MAX_CHARS,
        "事件文本按上限截断"
    );
    assert!(projected.chars().count() < text.chars().count());

    let message =
        acp_core::model::MessageId::new("40000000-0000-4000-8000-000000000077").expect("message");
    let turn = acp_core::model::TurnId::new("33333333-3333-4333-8333-000000000077").expect("turn");
    let request = RequestId::new("66666666-6666-4666-8666-000000000077").expect("request");
    let accepted = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: vec![acp_core::ports::TurnChange::Create(
                acp_core::ports::NewTurn {
                    turn: turn.clone(),
                    state: TurnState::Queued,
                    causation: Some(request.clone()),
                    started_at: None,
                },
            )],
            events: vec![PendingEvent::new(
                EventKind::Delta,
                EventType::new("user.message.delta").expect("event type"),
                StoredPolicy::ShortTerm,
                acp_core::model::EventPayload::new(
                    ViewJson::new(
                        &serde_json::json!({
                            "messageId": message.as_str(),
                            "deltaIndex": "0",
                            "text": projected,
                        })
                        .to_string(),
                    )
                    .expect("view"),
                    None,
                ),
                acp_core::model::EventOrigin::Device,
                Some(turn.clone()),
                Some(request.clone()),
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: acp_core::model::Actor::Device {
                    device: acp_core::model::DeviceId::new("55555555-5555-4555-8555-000000000077")
                        .expect("device"),
                },
                request: request.clone(),
                command: "session.prompt".to_owned(),
                kind: CommandKind::Mutation,
                session: Some(session.clone()),
                expected_version: None,
                request_fingerprint: Digest::new(&digest_text("long-input")).expect("digest"),
                accepted_at: at(1),
            }),
            command_terminal: None,
            origin_epoch: Some(epoch()),
            input: Some(acp_core::model::UserInputWrite {
                message: message.clone(),
                session: session.clone(),
                turn: turn.clone(),
                request,
                content: content.clone(),
                at: at(1),
            }),
        })
        .await
        .expect("commit long input");
    let delta = accepted.appended[0].clone();

    // 压缩这条 delta：正文权威仍是 owned_message。
    let mut compaction = commit(&session, 2);
    compaction.events.push(pending(
        EventKind::Summary,
        "turn.delta_compacted",
        r#"{"deltaCount":1,"turnId":"33333333-3333-4333-8333-000000000077"}"#,
    ));
    let server_epoch = ServerEpoch::new(&store_epoch(&dir).await).expect("epoch");
    compaction.compacted = vec![GlobalCursor::new(server_epoch, delta.global_sequence)];
    store.commit(compaction).await.expect("compact");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let stored_text: String = sqlx::query_scalar(
        "SELECT json_extract(payload_json, '$.text') FROM owned_event \
         WHERE event_type = 'user.message.delta'",
    )
    .fetch_one(&pool)
    .await
    .expect("event text");
    assert_eq!(
        stored_text.chars().count(),
        acp_core::model::PROMPT_MESSAGE_TEXT_MAX_CHARS,
        "事件文本按上限截断"
    );
    let compacted: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM owned_event WHERE event_type = 'user.message.delta' \
         AND compacted_into IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("compacted");
    assert_eq!(compacted, 1);
    pool.close().await;

    // 正式读回：完整正文（不按事件文本截断）。
    let page = store
        .read_view()
        .await
        .expect("view")
        .read_session(acp_core::ports::HistoryQuery {
            origin_order: false,
            session: session.clone(),
            include: acp_core::ports::HistoryInclude {
                messages: true,
                ..acp_core::ports::HistoryInclude::default()
            },
            after: None,
            before_history: None,
            before_messages: None,
            messages_limit: ReplayLimit::new(10),
            limit: ReplayLimit::new(10),
        })
        .await
        .expect("read");
    assert_eq!(page.messages.len(), 1);
    assert_eq!(page.messages[0].message, message);
    assert_eq!(page.messages[0].content.as_str(), content.json().as_str());
    assert!(
        page.messages[0].content.as_str().contains(&text),
        "完整输入必须逐字读回"
    );
}
