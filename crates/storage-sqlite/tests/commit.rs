//! §9.2(b)(c)、§9.3、§9.4、§9.9、§9.10、§9.11：`commit` 的事务性、幂等、序号、摘要与交互仲裁。

mod support;

use acp_core::model::{
    Actor, AgentId, AgentRef, CommandKind, CommandResult, CommandStatus, CommandTerminalRecord,
    ConflictKind, Digest, EntityRef, EventKind, EventOrigin, EventType, InteractionId,
    OriginCursor, OriginEpoch, PendingEvent, PortError, PublicError, RequestId, Sequence,
    SessionId, SessionState, StoredPolicy, Timestamp, TurnId, TurnState, ViewJson,
};
use acp_core::ports::{
    HistoryInclude, HistoryQuery, IdempotencyRecord, ModeChange, NewTurn, OwnedCommit, ReplayLimit,
    SessionQuery, SessionStore, SessionUpdate, StateChange, TurnChange,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const ORIGIN_EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
const REQUEST: &str = "11111111-1111-4111-8111-111111111111";
const REQUEST_OTHER: &str = "22222222-2222-4222-8222-222222222222";
const TURN: &str = "33333333-3333-4333-8333-333333333333";
const INTERACTION: &str = "44444444-4444-4444-8444-444444444444";

fn at(offset_hours: i64) -> Timestamp {
    // 固定基准 + 整小时偏移（UTC、毫秒、Z 的固定宽度）。
    Timestamp::new(&format!("2026-09-18T{offset_hours:02}:00:00.000Z")).expect("timestamp")
}

fn agent() -> AgentRef {
    AgentRef::try_new(
        AgentId::new("probe-agent").expect("agent id"),
        "Probe Agent",
    )
    .expect("agent ref")
}

fn actor() -> Actor {
    Actor::Device {
        device: acp_core::model::DeviceId::new("55555555-5555-4555-8555-555555555555")
            .expect("device id"),
        scopes: acp_core::model::ScopeSet::empty(),
    }
}

fn request(text: &str) -> RequestId {
    RequestId::new(text).expect("request id")
}

/// 规范摘要（`Digest` 要求 43 字符无填充 base64url，且末字符只有 4 位有效）。
fn fingerprint(seed: &str) -> Digest {
    Digest::new(&support::digest_text(seed)).expect("digest")
}

fn origin_epoch() -> OriginEpoch {
    OriginEpoch::new(ORIGIN_EPOCH).expect("origin epoch")
}

fn event(
    kind: EventKind,
    event_type: &str,
    view: &str,
    causation: Option<RequestId>,
) -> PendingEvent {
    PendingEvent::new(
        kind,
        EventType::new(event_type).expect("event type"),
        StoredPolicy::Durable,
        acp_core::model::EventPayload::new(ViewJson::new(view).expect("view json"), None),
        EventOrigin::Agent,
        None,
        causation,
    )
}

fn create_session(title: &str) -> OwnedCommit {
    OwnedCommit {
        session: None,
        at: at(0),
        expected_version: None,
        state: Some(StateChange::Create(acp_core::ports::NewSession {
            title: Some(title.to_owned()),
            agent: agent(),
        })),
        turns: Vec::new(),
        events: Vec::new(),
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: None,
        command_terminal: None,
        origin_epoch: Some(origin_epoch()),
    }
}

fn idempotency(text: &str, seed: &str, session: Option<SessionId>) -> IdempotencyRecord {
    IdempotencyRecord {
        actor: actor(),
        request: request(text),
        command: "session.prompt".to_owned(),
        kind: CommandKind::Mutation,
        session,
        expected_version: None,
        request_fingerprint: fingerprint(seed),
        accepted_at: at(0),
    }
}

async fn store(dir: &std::path::Path) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at(0))
        .await
        .expect("open store")
}

/// §9.2(b)：同一次提交里的状态 + turn + 事件 + 幂等 + 终态要么全落库、要么一行都不落。
#[tokio::test]
async fn commit_writes_state_turns_events_and_terminal_in_one_transaction() {
    let dir = temp_dir("commit-atomic-ok");
    let store = store(&dir).await;

    let created = store
        .commit(create_session("first"))
        .await
        .expect("create session");
    let session = created.session_id.expect("allocated session id");
    assert_eq!(created.version, acp_core::model::Version::new(1));
    assert_eq!(created.origin_epoch, Some(origin_epoch()));

    let commit = OwnedCommit {
        session: Some(session.clone()),
        at: at(1),
        expected_version: Some(acp_core::model::Version::new(1)),
        state: Some(StateChange::Update(SessionUpdate {
            state: Some(SessionState::Running),
            mode: ModeChange::Unchanged,
            closed_at: None,
            interaction: None,
        })),
        turns: vec![TurnChange::Create(NewTurn {
            turn: TurnId::new(TURN).expect("turn id"),
            state: TurnState::Running,
            causation: Some(request(REQUEST)),
            started_at: Some(at(1)),
        })],
        events: vec![event(
            EventKind::Delta,
            "agent.message",
            r#"{"kind":"delta"}"#,
            Some(request(REQUEST)),
        )],
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: Some(idempotency(REQUEST, "request-one", Some(session.clone()))),
        command_terminal: None,
        origin_epoch: Some(origin_epoch()),
    };
    let first = store.commit(commit.clone()).await.expect("commit");
    assert_eq!(first.version, acp_core::model::Version::new(2));
    assert_eq!(first.appended.len(), 1);

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_turn").await,
        1
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_event").await,
        1
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_command").await,
        1
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_command WHERE status = 'accepted'"
        )
        .await,
        1
    );
    // queue_index 由存储层分配（`NewTurn` 不带它）。
    assert_eq!(
        scalar_i64(&pool, "SELECT queue_index FROM owned_turn").await,
        1
    );
    // §9.4：会话级序号与会话 origin 序号在同一事务内各自 +1。
    let row = sqlx::query("SELECT session_sequence, origin_sequence FROM owned_event")
        .fetch_one(&pool)
        .await
        .expect("event row");
    let session_sequence: i64 = sqlx::Row::try_get(&row, "session_sequence").expect("sequence");
    let origin_sequence: i64 = sqlx::Row::try_get(&row, "origin_sequence").expect("origin");
    assert_eq!(session_sequence, 1);
    assert_eq!(origin_sequence, 1);
    pool.close().await;

    // 幂等重放：不追加事件、不改状态。
    let replay = store.commit(commit).await.expect("replay");
    assert!(replay.appended.is_empty());
    assert!(replay.replayed.is_some());
    assert_eq!(replay.version, acp_core::model::Version::new(2));
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_event").await,
        1,
        "a replay must not append events"
    );
    pool.close().await;

    store.close().await;
}

/// §9.2(b)：第二次提交失败后，`owned_session`/`owned_turn`/`owned_event`/`owned_command` 与调用前逐行相同。
#[tokio::test]
async fn failed_commit_rolls_back_every_row() {
    let dir = temp_dir("commit-rollback");
    // 建会话先用默认上限；随后按「实测总量 + 半条事件」派生预算重开（不拍数字）。
    let store = SqliteStore::open(StorageConfig::new(&dir), &at(0))
        .await
        .expect("open");

    let created = store
        .commit(create_session("first"))
        .await
        .expect("create session");
    let session = created.session_id.expect("session id");

    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_pool(&path).await;
    let mut before = Vec::new();
    for table in [
        "owned_session",
        "owned_turn",
        "owned_event",
        "owned_command",
        "owned_interaction",
    ] {
        before.push((table, table_snapshot(&pool, table).await));
    }
    pool.close().await;

    // 派生预算：实测总量 + 半条事件（一条事件的 payload 约 1 KiB）→ 本次 8 条写入必然超限。
    store.close().await;
    let measured_pool = raw_pool(&path).await;
    let measured = measured_storage_bytes(&measured_pool).await;
    measured_pool.close().await;
    let mut config = StorageConfig::new(&dir);
    config.max_total_size_bytes = measured + 512;
    let store = SqliteStore::open(config, &at(2)).await.expect("reopen");

    let padding = "x".repeat(1024);
    let mut events = Vec::new();
    for index in 0..8 {
        events.push(event(
            EventKind::Delta,
            "agent.message",
            &format!(r#"{{"n":{index},"pad":"{padding}"}}"#),
            Some(request(REQUEST_OTHER)),
        ));
    }
    let failing = OwnedCommit {
        session: Some(session.clone()),
        at: at(2),
        expected_version: None,
        state: Some(StateChange::Update(SessionUpdate {
            state: Some(SessionState::Running),
            mode: ModeChange::Unchanged,
            closed_at: None,
            interaction: None,
        })),
        turns: vec![TurnChange::Create(NewTurn {
            turn: TurnId::new(TURN).expect("turn id"),
            state: TurnState::Running,
            causation: None,
            started_at: Some(at(2)),
        })],
        events,
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: Some(idempotency(REQUEST_OTHER, "request-two", Some(session))),
        command_terminal: None,
        origin_epoch: Some(origin_epoch()),
    };
    let error = store
        .commit(failing)
        .await
        .expect_err("capacity must reject the write");
    assert!(matches!(
        error,
        PortError::Unavailable(acp_core::model::UnavailableKind::StorageFull)
    ));

    let pool = raw_pool(&path).await;
    for (table, snapshot) in before {
        assert_eq!(
            table_snapshot(&pool, table).await,
            snapshot,
            "{table} must be rolled back to the pre-commit state"
        );
    }
    pool.close().await;
    store.close().await;
}

/// §9.3：指纹/`expected_version` 不同即冲突；不同 actor 用同一 `requestId` 是两条独立命令。
#[tokio::test]
async fn idempotency_conflicts_are_detected() {
    let dir = temp_dir("commit-idempotency");
    let store = store(&dir).await;

    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");

    let mut commit = OwnedCommit {
        session: Some(session.clone()),
        at: at(1),
        expected_version: None,
        state: None,
        turns: Vec::new(),
        events: vec![event(EventKind::Delta, "agent.message", "{}", None)],
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: Some(idempotency(REQUEST, "request-one", Some(session.clone()))),
        command_terminal: None,
        origin_epoch: Some(origin_epoch()),
    };
    store.commit(commit.clone()).await.expect("accept");

    // 指纹不同 → IdempotencyConflict。
    commit.idempotency = Some(idempotency(REQUEST, "request-other", Some(session.clone())));
    let error = store.commit(commit.clone()).await.expect_err("conflict");
    assert!(matches!(
        error,
        PortError::Conflict(ConflictKind::IdempotencyConflict)
    ));

    // 指纹相同但 command 不同 → 同样是冲突。
    let mut renamed = idempotency(REQUEST, "request-one", Some(session.clone()));
    renamed.command = "session.cancel".to_owned();
    commit.idempotency = Some(renamed);
    let error = store.commit(commit.clone()).await.expect_err("conflict");
    assert!(matches!(
        error,
        PortError::Conflict(ConflictKind::IdempotencyConflict)
    ));

    // 不同 actor 用同一 requestId → 独立的一行。
    commit.idempotency = Some(IdempotencyRecord {
        actor: Actor::LocalCli,
        request: request(REQUEST),
        command: "session.prompt".to_owned(),
        kind: CommandKind::Mutation,
        session: Some(session.clone()),
        expected_version: None,
        request_fingerprint: fingerprint("request-one"),
        accepted_at: at(1),
    });
    store.commit(commit).await.expect("independent command");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_command").await,
        2
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_command WHERE actor_kind = 'cli'"
        )
        .await,
        1
    );
    pool.close().await;
    store.close().await;
}

/// §9.3/§11.2：终态块不带 `idempotency`，靠终态事件的 `causation` 定位既有行。
#[tokio::test]
async fn terminal_commit_completes_the_accepted_command() {
    let dir = temp_dir("commit-terminal");
    let store = store(&dir).await;
    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");

    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(idempotency(REQUEST, "request-one", Some(session.clone()))),
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("accept");

    let terminal_event = event(
        EventKind::Structured,
        "command.completed",
        r#"{"requestId":"11111111-1111-4111-8111-111111111111"}"#,
        Some(request(REQUEST)),
    );
    let terminal = CommandTerminalRecord::try_new(
        CommandStatus::Completed,
        Some(at(2)),
        None,
        Some(CommandResult::from_json_text(r#"{"ok":true}"#).expect("result")),
        None,
    )
    .expect("terminal record");

    let outcome = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(2),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![terminal_event],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: Some(terminal),
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("terminal");
    assert!(outcome.replayed.is_none());
    assert_eq!(outcome.appended.len(), 1);

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let row =
        sqlx::query("SELECT status, result_json, error_code, terminal_event_id FROM owned_command")
            .fetch_one(&pool)
            .await
            .expect("command row");
    assert_eq!(
        sqlx::Row::try_get::<String, _>(&row, "status").expect("status"),
        "completed"
    );
    assert_eq!(
        sqlx::Row::try_get::<Option<String>, _>(&row, "result_json").expect("result"),
        Some(r#"{"ok":true}"#.to_owned())
    );
    assert_eq!(
        sqlx::Row::try_get::<Option<String>, _>(&row, "terminal_event_id").expect("event"),
        Some(outcome.appended[0].id.as_str().to_owned())
    );
    // §9.10：每条非 accepted 的命令行都有对应终态事件行。
    let dangling = scalar_i64(
        &pool,
        "SELECT COUNT(*) FROM owned_command c WHERE c.status <> 'accepted' AND c.terminal_event_id IS NOT NULL \
         AND NOT EXISTS (SELECT 1 FROM owned_event e WHERE e.event_id = c.terminal_event_id)",
    )
    .await;
    assert_eq!(dangling, 0);
    pool.close().await;

    // 重复投递同一终态块：不追加事件、不改行，按重放回报既有记录。
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let events_before = table_snapshot(&pool, "owned_event").await;
    pool.close().await;
    let outcome = store
        .commit(OwnedCommit {
            session: Some(session),
            at: at(3),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(
                EventKind::Structured,
                "command.completed",
                "{}",
                Some(request(REQUEST)),
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: Some(
                CommandTerminalRecord::try_new(
                    CommandStatus::Completed,
                    Some(at(3)),
                    None,
                    None,
                    None,
                )
                .expect("terminal"),
            ),
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("duplicate terminal");
    assert!(outcome.replayed.is_some());
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        table_snapshot(&pool, "owned_event").await,
        events_before,
        "a duplicate terminal chunk must not append events"
    );
    pool.close().await;
    store.close().await;
}

/// 终态块指向不存在的命令 → `NotFound(Command)`。
#[tokio::test]
async fn terminal_for_unknown_request_is_not_found() {
    let dir = temp_dir("commit-terminal-missing");
    let store = store(&dir).await;
    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");

    let error = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(2),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(
                EventKind::Structured,
                "command.failed",
                "{}",
                Some(request(REQUEST)),
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: Some(
                CommandTerminalRecord::try_new(
                    CommandStatus::Failed,
                    Some(at(2)),
                    None,
                    None,
                    Some(PublicError::coded("internal.error", "boom", false).expect("error")),
                )
                .expect("terminal"),
            ),
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect_err("missing command");
    match error {
        PortError::NotFound(EntityRef::Command { session: found, .. }) => {
            assert_eq!(found.as_ref(), Some(&session))
        }
        other => panic!("expected NotFound(Command), got {other}"),
    }
    store.close().await;
}

/// §9.4：全局序号唯一递增；非会话级事件两个会话级序号为 NULL 但仍参与重放。
#[tokio::test]
async fn sequences_are_dense_and_global_sequence_covers_node_events() {
    let dir = temp_dir("commit-sequences");
    let store = store(&dir).await;
    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");

    let mut events = Vec::new();
    for index in 0..3 {
        events.push(event(
            EventKind::Delta,
            "agent.message",
            &format!(r#"{{"n":{index}}}"#),
            None,
        ));
    }
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events,
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("session events");

    // 非会话级事件：`session`/`session_sequence` 为 NULL，但仍分配 global_sequence。
    let node_event = store
        .commit(OwnedCommit {
            session: None,
            at: at(2),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(
                EventKind::State,
                "device.revoked",
                r#"{"deviceId":"55555555-5555-4555-8555-555555555555"}"#,
                None,
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        })
        .await
        .expect("node event");
    assert_eq!(node_event.appended.len(), 1);
    assert!(node_event.appended[0].session.is_none());
    assert!(node_event.appended[0].session_sequence.is_none());

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    // 会话内 session_sequence 无空洞：1,2,3。
    let sequences: Vec<i64> =
        sqlx::query_scalar("SELECT session_sequence FROM owned_event WHERE session_id IS NOT NULL ORDER BY session_sequence")
            .fetch_all(&pool)
            .await
            .expect("sequences");
    assert_eq!(sequences, vec![1, 2, 3]);
    // (session_id, origin_epoch, origin_sequence) 唯一。
    let duplicates = scalar_i64(
        &pool,
        "SELECT COUNT(*) FROM (SELECT session_id, origin_epoch, origin_sequence, COUNT(*) c \
         FROM owned_event WHERE session_id IS NOT NULL GROUP BY 1,2,3 HAVING c > 1)",
    )
    .await;
    assert_eq!(duplicates, 0);
    // 非会话级事件：两个会话级列为 NULL。
    let nulls = scalar_i64(
        &pool,
        "SELECT COUNT(*) FROM owned_event WHERE session_id IS NULL AND session_sequence IS NULL \
         AND event_type = 'device.revoked'",
    )
    .await;
    assert_eq!(nulls, 1);
    pool.close().await;

    // 重放里必须能看到非会话级事件（§9.4）。
    let view = store.read_view().await.expect("read view");
    let batch = view
        .replay(None, ReplayLimit::default())
        .await
        .expect("replay");
    assert_eq!(batch.events.len(), 4);
    let has_node_event = batch.events.iter().any(|delivery| match delivery {
        acp_core::model::CommittedDelivery::Owned(event) => {
            event.session.is_none() && event.session_sequence.is_none()
        }
        _ => false,
    });
    assert!(has_node_event);
    drop(view);
    store.close().await;
}

/// §9.9：`payload_digest` 是**对 `payload_json` 施 ACPR-CJ1 后的** SHA-256。
///
/// 关键反例：成员顺序被打乱的 view —— `payload_json` 按 §9.16 原样保留调用方字节，所以
/// `SHA256(原文)` 与 `SHA256(ACPR-CJ1(原文))` 必须不同，而入库的摘要必须等于后者。
#[tokio::test]
async fn payload_digest_is_the_cj1_digest_of_the_stored_payload() {
    use base64::Engine as _;
    use sha2::Digest as _;

    const CANONICAL: &str = r#"{"a":1,"b":"β"}"#;
    const REORDERED: &str = r#"{"b":"β","a":1}"#;

    let dir = temp_dir("commit-digest");
    let store = store(&dir).await;
    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");

    let mut events = Vec::new();
    for view in [CANONICAL, REORDERED] {
        events.push(event(EventKind::Delta, "agent.message", view, None));
    }
    let written = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events,
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("commit");
    assert_eq!(written.appended.len(), 2);

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let base64 = |bytes: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    for (index, original) in [CANONICAL, REORDERED].into_iter().enumerate() {
        let row =
            sqlx::query("SELECT payload_json, payload_digest FROM owned_event WHERE event_id = ?1")
                .bind(written.appended[index].id.as_str())
                .fetch_one(&pool)
                .await
                .expect("event row");
        let stored: String = sqlx::Row::try_get(&row, "payload_json").expect("payload");
        let digest: String = sqlx::Row::try_get(&row, "payload_digest").expect("digest");
        assert_eq!(stored, original, "the view must be stored byte-for-byte");

        let canonical = acpr_wire::cj1::canonicalize(&stored).expect("canonical");
        let expected = base64(&sha2::Sha256::digest(canonical.as_bytes()));
        assert_eq!(
            digest, expected,
            "payload_digest must be SHA256(ACPR-CJ1(payload_json))"
        );
        let raw = base64(&sha2::Sha256::digest(stored.as_bytes()));
        if stored == canonical {
            assert_eq!(
                digest, raw,
                "a canonical view hashes identically either way"
            );
        } else {
            assert_ne!(
                digest, raw,
                "a reordered view must NOT hash to SHA256(raw bytes)"
            );
        }
    }
    pool.close().await;
    store.close().await;
}

/// §7.3/§9.9：`payload_json` 不规范时（重复键、浮点）整事务拒绝且**零写入**，错误消息不带正文。
#[tokio::test]
async fn non_canonical_payload_is_rejected_without_writing() {
    use acp_core::model::ViewJson;

    let dir = temp_dir("commit-digest-invalid");
    let store = store(&dir).await;
    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");

    for view in [r#"{"a":1,"a":2}"#, r#"{"a":1.5}"#] {
        // 两个 view 都必须是 core 能接受的合法 JSON 文本——否则这条用例会“绕过”存储层的
        // ACPR-CJ1 拒绝路径而变成空断言。
        assert!(
            ViewJson::new(view).is_ok(),
            "core must accept `{view}` as JSON so the storage-side CJ1 rejection is exercised"
        );
        let error = store
            .commit(OwnedCommit {
                session: Some(session.clone()),
                at: at(2),
                expected_version: None,
                state: None,
                turns: Vec::new(),
                events: vec![event(EventKind::Delta, "agent.message", view, None)],
                interactions: Vec::new(),
                compacted: Vec::new(),
                idempotency: None,
                command_terminal: None,
                origin_epoch: Some(origin_epoch()),
            })
            .await
            .expect_err("a non-canonical view must be rejected");
        assert!(matches!(error, PortError::InvalidRequest(_)));
        assert!(
            !error.to_string().contains("a\":1"),
            "the error message must not echo the payload: {error}"
        );
    }

    // 零写入：只有会话行，没有事件行。
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_event").await,
        0,
        "events of a rejected commit must not be persisted"
    );
    pool.close().await;
    store.close().await;
}

/// §9.11：交互解析 first-writer-wins，第二个应答返回 `AlreadyResolved` 且不覆盖既有结果。
#[tokio::test]
async fn interaction_resolution_is_first_writer_wins() {
    use acp_core::model::{InteractionResolution, PermissionDecision, PermissionDecisionKind};

    let dir = temp_dir("commit-interaction");
    let store = store(&dir).await;
    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");

    // 待解析行由交互事件伴随写入；§5.2 的 `OwnedCommit` 没有创建字段，因此测试直接落一行
    // （`owned_interaction` 的列集合是合同冻结的，见交付报告的合同缺口说明）。
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    sqlx::query(
        "INSERT INTO owned_interaction (interaction_id, session_id, kind, state, created_at) \
         VALUES (?1, ?2, 'permission', 'pending', ?3)",
    )
    .bind(INTERACTION)
    .bind(session.as_str())
    .bind(AT)
    .execute(&pool)
    .await
    .expect("seed pending interaction");
    pool.close().await;

    let resolution = |option: &str, actor: Actor| {
        StateChange::Update(SessionUpdate {
            state: None,
            mode: ModeChange::Unchanged,
            closed_at: None,
            interaction: Some(acp_core::ports::InteractionResolved {
                interaction: InteractionId::new(INTERACTION).expect("interaction id"),
                resolution: InteractionResolution::Permission(
                    PermissionDecision::try_new(option, PermissionDecisionKind::AllowOnce)
                        .expect("decision"),
                ),
                resolved_by: actor,
            }),
        })
    };

    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: Some(resolution("allow-once", actor())),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("first writer wins");

    let second = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(2),
            expected_version: None,
            state: Some(resolution("reject-once", Actor::LocalCli)),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect_err("second writer must be rejected");
    assert!(matches!(
        second,
        PortError::Conflict(ConflictKind::AlreadyResolved)
    ));

    let pool = raw_pool(&path).await;
    let row = sqlx::query(
        "SELECT state, decision_option_id, resolved_at, resolved_by_kind FROM owned_interaction",
    )
    .fetch_one(&pool)
    .await
    .expect("interaction row");
    assert_eq!(
        sqlx::Row::try_get::<String, _>(&row, "state").expect("state"),
        "resolved"
    );
    assert_eq!(
        sqlx::Row::try_get::<Option<String>, _>(&row, "decision_option_id").expect("decision"),
        Some("allow-once".to_owned()),
        "the first decision must not be overwritten"
    );
    assert_eq!(
        sqlx::Row::try_get::<Option<String>, _>(&row, "resolved_at").expect("resolved_at"),
        Some(at(1).as_str().to_owned())
    );
    assert_eq!(
        sqlx::Row::try_get::<Option<String>, _>(&row, "resolved_by_kind").expect("actor kind"),
        Some("device".to_owned())
    );
    pool.close().await;

    // 不存在的 interactionId → NotFound。
    let missing = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(3),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: Some(acp_core::ports::InteractionResolved {
                    interaction: InteractionId::new("99999999-9999-4999-8999-999999999999")
                        .expect("interaction id"),
                    resolution: InteractionResolution::Permission(
                        PermissionDecision::try_new(
                            "allow-once",
                            PermissionDecisionKind::AllowOnce,
                        )
                        .expect("decision"),
                    ),
                    resolved_by: actor(),
                }),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect_err("unknown interaction");
    assert!(matches!(missing, PortError::NotFound(_)));

    store.close().await;
}

/// §9.10：外键与唯一键的悬空行数为 0。
#[tokio::test]
async fn references_are_intact() {
    let dir = temp_dir("commit-integrity");
    let store = store(&dir).await;
    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: vec![TurnChange::Create(NewTurn {
                turn: TurnId::new(TURN).expect("turn id"),
                state: TurnState::Running,
                causation: None,
                started_at: Some(at(1)),
            })],
            events: vec![event(EventKind::Delta, "agent.message", "{}", None)],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(idempotency(REQUEST, "request-one", Some(session.clone()))),
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("commit");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_event e WHERE e.turn_id IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM owned_turn t WHERE t.turn_id = e.turn_id)",
        )
        .await,
        0
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_attachment_link l WHERE NOT EXISTS \
             (SELECT 1 FROM owned_attachment a WHERE a.sha256 = l.sha256)",
        )
        .await,
        0
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_interaction i WHERE i.request_event IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM owned_event e WHERE e.global_sequence = i.request_event)",
        )
        .await,
        0
    );
    // (actor_kind, actor_id, request_id) 唯一由索引保证；这里确认约束确实生效。
    let duplicate = sqlx::query(
        "INSERT INTO owned_command (actor_kind, actor_id, request_id, command, kind, \
         request_fingerprint, accepted_at, status) \
         VALUES ('device', '55555555-5555-4555-8555-555555555555', ?1, 'session.prompt', \
         'mutation', ?2, ?3, 'accepted')",
    )
    .bind(REQUEST)
    .bind(support::digest_text("request-one"))
    .bind(AT)
    .execute(&pool)
    .await;
    assert!(duplicate.is_err(), "idempotency key must be unique");
    pool.close().await;
    store.close().await;
}

/// §9.10/§9.4：`PRAGMA integrity_check = ok`（正常路径）。
#[tokio::test]
async fn integrity_check_is_ok_after_normal_writes() {
    let dir = temp_dir("commit-integrity-ok");
    let store = store(&dir).await;
    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");
    store
        .commit(OwnedCommit {
            session: Some(session),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(EventKind::Delta, "agent.message", "{}", None)],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("commit");
    store.close().await;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let check: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .expect("integrity_check");
    assert_eq!(check, "ok");
    pool.close().await;
}

/// §9.2(c)：进程被强杀后 `PRAGMA integrity_check = ok`，且终态命令行都有对应终态事件。
///
/// 子进程（同一个测试二进制，`--exact crash_child`）提交一个会话 + 事件 + 终态命令后
/// `std::process::abort()`，不执行任何析构；父进程随后重新打开库并检查。
#[tokio::test]
async fn crash_recovery_leaves_an_consistent_database() {
    if std::env::var("ACPR_CRASH_DIR").is_ok() {
        return; // 父进程路径
    }
    let dir = temp_dir("commit-crash");
    let status = std::process::Command::new(std::env::current_exe().expect("current exe"))
        .args([
            "--exact",
            "crash_child",
            "--nocapture",
            "--ignored",
            "--test-threads=1",
        ])
        .env("ACPR_CRASH_DIR", dir.display().to_string())
        .status()
        .expect("spawn crash child");
    assert!(!status.success(), "the child must die without unwinding");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let check: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .expect("integrity_check");
    assert_eq!(check, "ok");
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_session").await,
        1
    );
    // §9.2(c) / §7.3：每条**到达终态**的命令行都必须有指向存在事件的 `terminal_event_id`。
    // 判据来自 §7.3 自己的 CHECK（`failed`/`uncertain` 必有终态事件；`completed` 的 mutation 同理），
    // 而不是「status <> accepted」那种被弱化过的写法。
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_command c WHERE \
             (c.status IN ('failed','uncertain') \
              OR (c.kind = 'mutation' AND c.status NOT IN ('accepted','rejected'))) \
             AND (c.terminal_event_id IS NULL OR NOT EXISTS \
                  (SELECT 1 FROM owned_event e WHERE e.event_id = c.terminal_event_id))",
        )
        .await,
        0,
        "every command that reached a terminal state must publish its terminal event"
    );
    pool.close().await;
}

/// 子进程：写入后立即 `abort()`（硬杀，无析构、无检查点）。
#[tokio::test]
#[ignore = "由 crash_recovery_leaves_an_consistent_database 拉起"]
async fn crash_child() {
    let dir = std::env::var("ACPR_CRASH_DIR").expect("ACPR_CRASH_DIR");
    let dir = std::path::PathBuf::from(dir);
    let store = SqliteStore::open(StorageConfig::new(&dir), &at(0))
        .await
        .expect("open");
    let created = store.commit(create_session("crash")).await.expect("create");
    let session = created.session_id.expect("session id");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(EventKind::Delta, "agent.message", "{}", None)],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(idempotency(REQUEST, "request-one", Some(session.clone()))),
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("accept");
    store
        .commit(OwnedCommit {
            session: Some(session),
            at: at(2),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(
                EventKind::Structured,
                "command.completed",
                "{}",
                Some(request(REQUEST)),
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: Some(
                CommandTerminalRecord::try_new(
                    CommandStatus::Completed,
                    Some(at(2)),
                    None,
                    Some(CommandResult::from_json_text(r#"{"ok":true}"#).expect("result")),
                    None,
                )
                .expect("terminal"),
            ),
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("terminal");
    // 硬杀：不析构、不 checkpoint、不 close。
    std::process::abort();
}

/// 列表/载入/历史/保留窗口：`state` 为 `None` 时不改状态，`Version` 每次提交 +1。
#[tokio::test]
async fn session_update_keeps_state_when_absent_and_bumps_version() {
    let dir = temp_dir("commit-update-semantics");
    let store = store(&dir).await;
    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");

    // 只解析交互的提交（`state: None`）不得改状态，但版本 +1。
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: Some(acp_core::model::Version::new(1)),
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect("update");

    let loaded = store
        .load(&session)
        .await
        .expect("load")
        .expect("session exists");
    assert_eq!(loaded.session.state(), SessionState::Idle);
    assert_eq!(loaded.session.version(), acp_core::model::Version::new(2));
    assert_eq!(loaded.origin_epoch, origin_epoch());
    assert_eq!(loaded.head.global_sequence, Sequence::new(0).expect("zero"));

    // 过期版本 → VersionMismatch。
    let stale = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(2),
            expected_version: Some(acp_core::model::Version::new(1)),
            state: Some(StateChange::Update(SessionUpdate {
                state: Some(SessionState::Running),
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(origin_epoch()),
        })
        .await
        .expect_err("stale version");
    assert!(matches!(
        stale,
        PortError::Conflict(ConflictKind::VersionMismatch)
    ));

    // 不存在的会话 → NotFound。
    let missing = store
        .commit(OwnedCommit {
            session: Some(SessionId::new("77777777-7777-4777-8777-777777777777").expect("id")),
            at: at(3),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: Some(SessionState::Running),
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        })
        .await
        .expect_err("missing session");
    assert!(matches!(missing, PortError::NotFound(_)));

    // 列表 / 事件类型 / 历史页。
    let listed = store
        .list(SessionQuery::default())
        .await
        .expect("list sessions");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].session_id(), &session);
    assert_eq!(*listed[0].origin(), acp_core::model::ResourceOrigin::Local);

    let view = store.read_view().await.expect("read view");
    let page = view
        .read_session(HistoryQuery {
            session: session.clone(),
            include: HistoryInclude {
                messages: true,
                turns: true,
                pending_interactions: true,
                config_options: true,
                capabilities: true,
            },
            after: None,
            limit: ReplayLimit::default(),
        })
        .await
        .expect("history");
    assert!(page.config.is_empty(), "the store keeps no live config");
    assert!(
        page.capabilities.is_none(),
        "the store keeps no capabilities"
    );
    assert_eq!(page.head, view.head().await.expect("view head"));
    drop(view);

    // 保留窗口：没有事件时为 None。
    assert!(
        store
            .retention_window(&session)
            .await
            .expect("window")
            .is_none()
    );

    let health = store.health().await.expect("health");
    assert!(health.integrity_ok);
    assert!(!health.read_only);
    assert_eq!(health.user_version, 3);

    let found = store
        .find_request(&request(REQUEST), &actor())
        .await
        .expect("find");
    assert!(found.is_none());
    store.close().await;
}

/// WP5（任务 2.14）：`ReadView::node_link_slice` 在同一次读视图里给出会话摘要、origin head、
/// 未决交互（含创建事件 id）与 `after` 之后的 origin 事件**含正文**。
#[tokio::test]
async fn node_link_slice_returns_origin_events_and_pending_interactions() {
    let dir = temp_dir("commit-node-link-slice");
    let store = store(&dir).await;
    let created = store.commit(create_session("slice")).await.expect("create");
    let session = created.session_id.expect("session id");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![
                event(
                    EventKind::State,
                    "session.state.changed",
                    r#"{"state":"idle"}"#,
                    None,
                ),
                event(
                    EventKind::FinalMessage,
                    "agent.message.completed",
                    r#"{"messageId":"m1"}"#,
                    None,
                ),
            ],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(idempotency(REQUEST, "slice", None)),
            command_terminal: None,
            origin_epoch: None,
        })
        .await
        .expect("commit events");

    // 未决交互行直接落库（`OwnedCommit` 没有创建字段，§6 第 13 条的已知合同缺口）：
    // `request_event` 指向第二条事件，切片必须能把它的 `event_id` 带回来。
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    let second_event: i64 = sqlx::query_scalar(
        "SELECT global_sequence FROM owned_event WHERE event_type = 'agent.message.completed'",
    )
    .fetch_one(&pool)
    .await
    .expect("second event");
    sqlx::query(
        "INSERT INTO owned_interaction (interaction_id, session_id, kind, request_event, \
         state, created_at) VALUES (?1, ?2, 'permission', ?3, 'pending', ?4)",
    )
    .bind(INTERACTION)
    .bind(session.as_str())
    .bind(second_event)
    .bind(AT)
    .execute(&pool)
    .await
    .expect("seed pending interaction");
    pool.close().await;

    let requested = SessionId::new("11111111-1111-4111-8111-111111111111").expect("session");
    assert_ne!(session, requested, "会话 id 由存储层分配");

    let requested = SessionId::new(REQUEST).expect("session");
    assert_ne!(session, requested, "会话 id 由存储层分配");

    let view = store.read_view().await.expect("read view");
    let slice = view
        .node_link_slice(&session, None, ReplayLimit::new(10))
        .await
        .expect("slice");
    assert_eq!(slice.summary.session_id(), &session);
    assert_eq!(slice.summary.state(), SessionState::Idle);
    assert_eq!(slice.events.len(), 2, "两条会话事件都在切片里");
    assert_eq!(
        slice.events[0].event.origin_sequence,
        Some(Sequence::new(1).expect("sequence"))
    );
    assert_eq!(
        slice.head.origin_sequence.get(),
        2,
        "head 是最大 origin 序列"
    );
    assert!(
        slice.events[0].payload.view.as_str().contains("state"),
        "事件正文与定位同源（来自同一次读视图）"
    );
    assert_eq!(slice.pending_interactions.len(), 1);
    assert_eq!(
        slice.pending_interactions[0].interaction.id().as_str(),
        INTERACTION
    );
    assert!(
        !slice.pending_interactions[0]
            .origin_event
            .as_str()
            .is_empty(),
        "创建事件 id 必须被带回（wire 的 payloadDigest 需要它的 payload）"
    );

    // `after` 按 origin 序列前进：只取该点之后的事件。
    let after = OriginCursor {
        origin_epoch: slice.head.origin_epoch.clone(),
        origin_sequence: Sequence::new(1).expect("sequence"),
    };
    let resumed = view
        .node_link_slice(&session, Some(after), ReplayLimit::new(10))
        .await
        .expect("resumed slice");
    assert_eq!(resumed.events.len(), 1);
    assert_eq!(
        resumed.events[0].event.event_type.as_str(),
        "agent.message.completed"
    );

    // 会话不存在 → NotFound。
    let unknown = SessionId::new(REQUEST).expect("session");
    assert!(matches!(
        view.node_link_slice(&unknown, None, ReplayLimit::new(1))
            .await,
        Err(PortError::NotFound(_))
    ));

    // 读视图持有一个显式只读事务：先释放它再关池，否则 `close()` 会等一个永不结束的连接。
    drop(view);
    store.close().await;
}

/// §9.14 / §7.3：`owned_audit` 的列集合与冻结清单**逐项相等**（新增内容列即失败）。
///
/// 审计行不得含内容（`SECURITY_DESIGN.md` §14.2），所以「列集合相等」就是这条判据的可观察形式；
/// `action` 的取值域由 `enum_coverage.rs::ddl_enum_lists_match_the_core_enums` 单独锁住。
#[tokio::test]
async fn owned_audit_has_the_frozen_column_list() {
    let dir = temp_dir("commit-audit-columns");
    let store = store(&dir).await;
    store.close().await;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        column_names(&pool, "owned_audit").await,
        vec![
            "audit_id",
            "at",
            "action",
            "actor_kind",
            "actor_id",
            "via_node_id",
            "local_principal_ref",
            "target_kind",
            "target_id",
            "outcome",
            "detail_digest",
        ],
        "owned_audit must match §7.3 exactly (no content column may be added)"
    );
    pool.close().await;
}

/// §9.3：幂等键相同、指纹相同但 `expected_version` 不同 → `IdempotencyConflict`；
/// 完全相同的重提 → 回放（`replayed.is_some()`）且事件数不变。
#[tokio::test]
async fn expected_version_is_part_of_the_idempotency_check() {
    let dir = temp_dir("commit-expected-version");
    let store = store(&dir).await;
    let created = store.commit(create_session("first")).await.expect("create");
    let session = created.session_id.expect("session id");

    let accepted = OwnedCommit {
        session: Some(session.clone()),
        at: at(1),
        expected_version: None,
        state: None,
        turns: Vec::new(),
        events: vec![event(EventKind::Delta, "agent.message", r#"{"n":1}"#, None)],
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: Some(idempotency(
            REQUEST,
            "expected-version",
            Some(session.clone()),
        )),
        command_terminal: None,
        origin_epoch: Some(origin_epoch()),
    };
    store.commit(accepted.clone()).await.expect("accept");

    // 完全相同的重提 → 回放。
    let replay = store.commit(accepted.clone()).await.expect("replay");
    assert!(replay.replayed.is_some());
    assert!(replay.appended.is_empty());

    // 只有 `expected_version` 不同（指纹相同）→ 冲突。
    let mut differing = accepted.clone();
    let mut record = differing.idempotency.clone().expect("record");
    record.expected_version = Some(acp_core::model::Version::new(1));
    differing.idempotency = Some(record);
    let error = store
        .commit(differing)
        .await
        .expect_err("expected_version must take part in the idempotency check");
    assert!(matches!(
        error,
        PortError::Conflict(ConflictKind::IdempotencyConflict)
    ));

    // §9.3 的两条结论都不得追加事件。
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_event").await,
        1
    );
    pool.close().await;
    store.close().await;
}
