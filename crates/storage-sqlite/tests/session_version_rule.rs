//! §6 第 19 条 / §9 判据 31 的存储侧：真实 SQLite 上的会话版本规则。
//!
//! core 在提交前按「含 `StateChange` 的提交 +1、否则不变」推导 `session.mode.changed` /
//! `session.config.changed` 的 `version`，提交后与 `CommitOutcome.version` 比对（不一致 → `PortError::Corrupt`
//! 失败关闭）。本文件用**真实存储层**断言这条规则成立，因此 core 的推导与断言在真实实现上不会误报；
//! 它不重复 core 的注入逻辑（那部分由 `crates/core` 的用例覆盖）。

mod support;

use acp_core::model::{
    AgentId, AgentRef, EventKind, EventOrigin, EventPayload, EventType, ModeId, ModeRef,
    OriginEpoch, PendingEvent, SessionId, SessionState, StoredPolicy, Timestamp, TurnId, TurnState,
    Version, ViewJson,
};
use acp_core::ports::{
    ModeChange, NewSession, NewTurn, OwnedCommit, SessionStore, SessionUpdate, StateChange,
    TurnChange,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const ORIGIN_EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
const TURN: &str = "33333333-3333-4333-8333-333333333333";

fn at(offset_hours: i64) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{offset_hours:02}:00:00.000Z")).expect("timestamp")
}

fn origin_epoch() -> OriginEpoch {
    OriginEpoch::new(ORIGIN_EPOCH).expect("origin epoch")
}

fn agent() -> AgentRef {
    AgentRef::try_new(
        AgentId::new("probe-agent").expect("agent id"),
        "Probe Agent",
    )
    .expect("agent ref")
}

/// 一条已按 §10.3 注入 `version` 的 `session.mode.changed`（core 的注入结果就是这种文本）。
fn mode_changed(version: u64) -> PendingEvent {
    PendingEvent::new(
        EventKind::State,
        EventType::new("session.mode.changed").expect("event type"),
        StoredPolicy::Durable,
        EventPayload::new(
            ViewJson::new(&format!(
                r#"{{"version":"{version}","currentModeId":"code"}}"#
            ))
            .expect("view json"),
            None,
        ),
        EventOrigin::Agent,
        None,
        None,
    )
}

fn create_session() -> OwnedCommit {
    OwnedCommit {
        session: None,
        at: at(0),
        expected_version: None,
        state: Some(StateChange::Create(NewSession {
            title: Some("version rule".to_owned()),
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

fn commit(
    session: &SessionId,
    at: Timestamp,
    expected_version: Option<Version>,
    state: Option<StateChange>,
    events: Vec<PendingEvent>,
) -> OwnedCommit {
    OwnedCommit {
        session: Some(session.clone()),
        at,
        expected_version,
        state,
        turns: Vec::new(),
        events,
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: None,
        command_terminal: None,
        origin_epoch: None,
    }
}

async fn store(dir: &std::path::Path) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at(0))
        .await
        .expect("open store")
}

/// 纯事件提交不递增版本：注入值（当前版本）与返回的 `version` 一致，正文逐字节保留。
#[tokio::test]
async fn event_only_commits_keep_the_session_version() {
    let dir = temp_dir("session-version-event-only");
    let store = store(&dir).await;
    let created = store.commit(create_session()).await.expect("create");
    let session = created.session_id.expect("session id");
    assert_eq!(created.version, Version::new(1));

    let event = mode_changed(1);
    let outcome = store
        .commit(commit(&session, at(1), None, None, vec![event]))
        .await
        .expect("event-only commit");
    assert_eq!(
        outcome.version,
        Version::new(1),
        "无状态变更的提交不递增版本（core 据此推导）"
    );
    let event_id = outcome.appended[0].id.clone();
    let payload = store
        .read_view()
        .await
        .expect("view")
        .event_payload(&event_id)
        .await
        .expect("payload")
        .expect("row exists");
    assert_eq!(
        payload.view.as_str(),
        r#"{"version":"1","currentModeId":"code"}"#,
        "存储层必须逐字节保留 view（注入由 core 完成）"
    );
    assert_eq!(
        store
            .load(&session)
            .await
            .expect("load")
            .expect("session")
            .session
            .version(),
        Version::new(1)
    );
    // 先显式关闭连接池再让临时目录守卫出作用域：Windows 上句柄未释放时目录删不掉。
    store.close().await;
}

/// 含状态变更的提交恰好 +1（两次连续状态变更各 +1，纯事件提交夹在中间不递增）。
#[tokio::test]
async fn state_change_commits_bump_the_session_version_by_one() {
    let dir = temp_dir("session-version-state-change");
    let store = store(&dir).await;
    let created = store.commit(create_session()).await.expect("create");
    let session = created.session_id.expect("session id");

    let mode_change = || {
        Some(StateChange::Update(SessionUpdate {
            state: None,
            mode: ModeChange::Set(
                ModeRef::try_new(ModeId::new("code").expect("mode id"), "Code").expect("mode ref"),
            ),
            closed_at: None,
            interaction: None,
        }))
    };

    let first = store
        .commit(commit(
            &session,
            at(1),
            None,
            mode_change(),
            vec![mode_changed(2)],
        ))
        .await
        .expect("first state change");
    assert_eq!(first.version, Version::new(2), "状态变更提交 +1");

    // 中间夹一次纯事件提交：版本停在上一次的值。
    let middle = store
        .commit(commit(&session, at(2), None, None, vec![mode_changed(2)]))
        .await
        .expect("event-only commit");
    assert_eq!(middle.version, Version::new(2));

    let second = store
        .commit(commit(
            &session,
            at(3),
            None,
            mode_change(),
            vec![mode_changed(3)],
        ))
        .await
        .expect("second state change");
    assert_eq!(second.version, Version::new(3), "状态变更提交再次 +1");
    // 先显式关闭连接池再让临时目录守卫出作用域：Windows 上句柄未释放时目录删不掉。
    store.close().await;
}

/// 状态变更提交上的 `expected_version` 必须等于当前版本：core 的推导在真实存储上是可校验的。
#[tokio::test]
async fn a_state_change_with_a_stale_expected_version_is_rejected() {
    let dir = temp_dir("session-version-stale-expected");
    let store = store(&dir).await;
    let created = store.commit(create_session()).await.expect("create");
    let session = created.session_id.expect("session id");

    let stale = store
        .commit(commit(
            &session,
            at(1),
            Some(Version::new(7)),
            Some(StateChange::Update(SessionUpdate {
                state: Some(SessionState::Running),
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
            })),
            Vec::new(),
        ))
        .await;
    assert!(
        matches!(
            stale,
            Err(acp_core::model::PortError::Conflict(
                acp_core::model::ConflictKind::VersionMismatch
            ))
        ),
        "过期的 expected_version 必须被拒绝：{stale:?}"
    );
    assert_eq!(
        store
            .load(&session)
            .await
            .expect("load")
            .expect("session")
            .session
            .version(),
        Version::new(1),
        "被拒绝的提交不改变版本"
    );
    // 未被拒绝的路径照常工作（turn 行与事件在同一事务内）。
    let outcome = store
        .commit(OwnedCommit {
            turns: vec![TurnChange::Create(NewTurn {
                turn: TurnId::new(TURN).expect("turn id"),
                state: TurnState::Queued,
                causation: None,
                started_at: Some(at(1)),
            })],
            ..commit(
                &session,
                at(1),
                Some(Version::new(1)),
                Some(StateChange::Update(SessionUpdate {
                    state: Some(SessionState::Queued),
                    mode: ModeChange::Unchanged,
                    closed_at: None,
                    interaction: None,
                })),
                Vec::new(),
            )
        })
        .await
        .expect("matching expected version");
    assert_eq!(outcome.version, Version::new(2));
    // 先显式关闭连接池再让临时目录守卫出作用域：Windows 上句柄未释放时目录删不掉。
    store.close().await;
}
