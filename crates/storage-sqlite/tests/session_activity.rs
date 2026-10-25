//! `session.activity.changed` 的落盘语义（`docs/CORE_PORTS_AND_STORAGE.md` §6 第 23 条、§7 不改
//! DDL）：复用既有 `owned_event` 表，事件可重放、带自己那条的持久正文，但**不是**会话时间线条目
//! （`SYNC_PROTOCOL.md` §10.2.1 的「不进入历史」表），也不改变任务三态事实。

mod support;

use acp_core::broker::{HISTORY_ENTRY_EVENT_TYPES, persistence_policy};
use acp_core::model::{
    AgentId, AgentRef, EventKind, EventOrigin, EventPayload, EventType, OriginEpoch, PendingEvent,
    Sequence, SessionId, SessionState, StoredPolicy, Timestamp, ViewJson,
};
use acp_core::ports::{
    HistoryInclude, HistoryQuery, NewSession, OwnedCommit, ReplayLimit, SessionStore, StateChange,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";

fn at(hour: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{hour:02}:00:00.000Z")).expect("timestamp")
}

fn agent() -> AgentRef {
    AgentRef::try_new(
        AgentId::new("probe-agent").expect("agent id"),
        "Probe Agent",
    )
    .expect("agent ref")
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
                title: Some("activity".to_owned()),
                agent: agent(),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(OriginEpoch::new(EPOCH).expect("epoch")),
            input: None,
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id")
}

fn activity_event(session: &SessionId, activity: &str) -> PendingEvent {
    let view = format!(
        r#"{{"session":{{"sessionId":"{}","title":null,"agent":{{"agentId":"probe-agent","name":"Probe Agent"}},"state":"idle","localActivity":"{activity}","origin":{{"kind":"local"}},"currentMode":null,"version":"1","createdAt":"2026-09-18T00:00:00.000Z","updatedAt":"2026-09-18T01:00:00.000Z","workspace":null}}}}"#,
        session.as_str()
    );
    PendingEvent::new(
        EventKind::State,
        EventType::new("session.activity.changed").expect("event type"),
        StoredPolicy::Durable,
        EventPayload::new(ViewJson::new(&view).expect("view"), None),
        EventOrigin::Daemon,
        None,
        None,
    )
}

#[tokio::test]
async fn activity_events_are_replayable_but_never_history_entries() {
    let dir = temp_dir("session-activity");
    let store = open(&dir).await;
    let session = create_session(&store).await;

    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![activity_event(&session, "active")],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(OriginEpoch::new(EPOCH).expect("epoch")),
            input: None,
        })
        .await
        .expect("commit activity event");

    // 历史条目集合不含该类型（常量即唯一筛选口径，§10.2.1）。
    assert!(!HISTORY_ENTRY_EVENT_TYPES.contains(&"session.activity.changed"));
    let event_type = EventType::new("session.activity.changed").expect("event type");
    assert_eq!(
        persistence_policy(&event_type),
        acp_core::model::PersistencePolicy::Durable
    );

    let view = store.read_view().await.expect("read view");
    let slice = view
        .sync_replay(None, ReplayLimit::default())
        .await
        .expect("replay");
    let record = slice
        .events
        .iter()
        .find(|record| record.event.event_type.as_str() == "session.activity.changed")
        .expect("activity event is replayable");
    assert_eq!(record.origin_kind, EventOrigin::Daemon);
    assert!(
        record
            .payload
            .view
            .as_str()
            .contains(r#""localActivity":"active""#)
    );

    // 历史页不带它；任务三态事实也不因生命周期投影改变。
    let page = view
        .read_session(HistoryQuery {
            origin_order: false,
            session: session.clone(),
            include: HistoryInclude {
                history: true,
                ..HistoryInclude::default()
            },
            before_history: None,
            before_messages: None,
            messages_limit: ReplayLimit::new(0),
            after: None,
            limit: ReplayLimit::new(32),
        })
        .await
        .expect("history page");
    assert!(
        page.history
            .iter()
            .all(|record| record.event.event_type.as_str() != "session.activity.changed"),
        "the activity projection is not a timeline entry"
    );
    assert!(page.events.is_empty(), "messages face is unaffected");

    let facts = store.session_activity(&session).await.expect("facts");
    assert_eq!(facts.unresolved_interactions, 0);
    assert_eq!(facts.unfinished_turns, 0);
    let snapshot = store
        .load(&session)
        .await
        .expect("load")
        .expect("session row");
    assert_eq!(snapshot.session.state(), SessionState::Idle);
    assert_eq!(
        snapshot.head.global_sequence,
        Sequence::new(1).expect("seq")
    );
}
