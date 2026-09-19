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
        .filter_map(|delivery| match delivery {
            acp_core::model::CommittedDelivery::Owned(event) => Some(event.global_sequence.get()),
            _ => None,
        })
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
