//! §9.7、§9.8：保留窗口、`expires_at` 清理、容量顺序与 `Unavailable(StorageFull)`。

mod support;

use acp_core::model::{
    AgentId, AgentRef, EventKind, EventOrigin, EventType, OriginEpoch, Sequence, StoredPolicy,
    Timestamp, ViewJson,
};
use acp_core::ports::{
    NewSession, OwnedCommit, PruneReport, ReplayLimit, ResetReason, RetentionPolicy, SessionStore,
    StateChange,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";

fn at(text: &str) -> Timestamp {
    Timestamp::new(text).expect("timestamp")
}

fn agent() -> AgentRef {
    AgentRef::try_new(
        AgentId::new("probe-agent").expect("agent id"),
        "Probe Agent",
    )
    .expect("agent ref")
}

fn epoch() -> OriginEpoch {
    OriginEpoch::new(EPOCH).expect("epoch")
}

fn event(
    kind: EventKind,
    event_type: &str,
    policy: StoredPolicy,
    view: &str,
) -> acp_core::model::PendingEvent {
    acp_core::model::PendingEvent::new(
        kind,
        EventType::new(event_type).expect("event type"),
        policy,
        acp_core::model::EventPayload::new(ViewJson::new(view).expect("view"), None),
        EventOrigin::Agent,
        None,
        None,
    )
}

async fn create(store: &SqliteStore, hour: u32) -> acp_core::model::SessionId {
    store
        .commit(OwnedCommit {
            session: None,
            at: at(&format!("2026-09-18T{hour:02}:00:00.000Z")),
            expected_version: None,
            state: Some(StateChange::Create(NewSession {
                title: Some("retention".to_owned()),
                agent: agent(),
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

async fn append(
    store: &SqliteStore,
    session: &acp_core::model::SessionId,
    hour: u32,
    events: Vec<acp_core::model::PendingEvent>,
) {
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(&format!("2026-09-18T{hour:02}:00:00.000Z")),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events,
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("append events");
}

fn policy() -> RetentionPolicy {
    RetentionPolicy {
        transcript_retention_days: 90,
        sync_event_retention_days: 7,
        audit_retention_days: 365,
        max_total_size_bytes: 2 * 1024 * 1024 * 1024,
        max_session_size_bytes: 100 * 1024 * 1024,
        persist_deltas: false,
    }
}

/// §9.7：到期行被清理、`retention_window` 前移、承诺期内的 delta 仍可重放。
#[tokio::test]
async fn prune_removes_expired_events_and_moves_the_window() {
    let dir = temp_dir("retention-window");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at("2026-09-18T00:00:00.000Z"))
        .await
        .expect("open");
    let session = create(&store, 0).await;
    append(
        &store,
        &session,
        1,
        vec![
            event(
                EventKind::Delta,
                "agent.message",
                StoredPolicy::ShortTerm,
                r#"{"d":1}"#,
            ),
            event(
                EventKind::FinalMessage,
                "agent.message",
                StoredPolicy::Durable,
                r#"{"m":1}"#,
            ),
            event(
                EventKind::State,
                "session.state",
                StoredPolicy::Durable,
                r#"{"s":1}"#,
            ),
        ],
    )
    .await;

    // 承诺期内：全部可重放。
    assert_eq!(
        store
            .retention_window(&session)
            .await
            .expect("window")
            .map(|(low, _)| low),
        Some(Sequence::new(1).expect("sequence"))
    );

    // 8 天后：delta（7 天窗口）到期，正文/状态（90 天）仍在。
    let report = store
        .prune(policy(), at("2026-09-26T00:00:00.000Z"))
        .await
        .expect("prune");
    assert_eq!(report.removed_events, 1, "only the expired delta goes");
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_event WHERE kind = 'delta'"
        )
        .await,
        0
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_event WHERE kind <> 'delta'"
        )
        .await,
        2
    );
    // §7.5：`last_prune_at` 随 prune 前进。
    assert_eq!(
        meta_rows(&pool)
            .await
            .iter()
            .find(|(key, _)| key == "last_prune_at")
            .map(|(_, value)| value.clone()),
        Some("2026-09-26T00:00:00.000Z".to_owned())
    );
    pool.close().await;

    // 窗口前移：下界变成 2（第一条仍在的重放序号）。
    assert_eq!(
        store.retention_window(&session).await.expect("window"),
        Some((
            Sequence::new(2).expect("sequence"),
            Sequence::new(3).expect("sequence")
        ))
    );

    // 游标必须带库里真实的 `serverEpoch`（由首次创建时生成，不是夹具常量）。
    let view = store.read_view().await.expect("view");
    let server_epoch = view.head().await.expect("head").server_epoch;

    // 游标 1 = 客户端已看到第 1 条，下一条（2）仍在窗口内 → 正常续读，不需要 reset。
    let resumable = view
        .replay(
            Some(acp_core::model::GlobalCursor::new(
                server_epoch.clone(),
                Sequence::new(1).expect("sequence"),
            )),
            ReplayLimit::default(),
        )
        .await
        .expect("replay");
    assert_eq!(resumable.reset_required, None);
    assert_eq!(resumable.events.len(), 2);

    // 游标 0 = 客户端什么都还没收到，而下一条（1）已被清理 → `cursor_expired`。
    let batch = view
        .replay(
            Some(acp_core::model::GlobalCursor::new(
                server_epoch,
                Sequence::new(0).expect("sequence"),
            )),
            ReplayLimit::default(),
        )
        .await
        .expect("replay");
    assert_eq!(batch.reset_required, Some(ResetReason::CursorExpired));
    assert_eq!(batch.events.len(), 2, "replay restarts from the window");
    drop(view);

    // 91 天后：90 天窗口的正文与状态事件也到期。
    let report = store
        .prune(policy(), at("2026-12-18T00:00:00.000Z"))
        .await
        .expect("prune");
    assert_eq!(report.removed_events, 2);
    assert!(
        store
            .retention_window(&session)
            .await
            .expect("window")
            .is_none()
    );
    store.close().await;
}

/// 会话的 `serverEpoch` 不匹配 → `sync.reset_required(epoch_mismatch)`；超过 head → 协议层拒绝。
#[tokio::test]
async fn cursor_validation_reports_the_documented_reasons() {
    let dir = temp_dir("retention-cursor");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at("2026-09-18T00:00:00.000Z"))
        .await
        .expect("open");
    let session = create(&store, 0).await;
    append(
        &store,
        &session,
        1,
        vec![event(
            EventKind::FinalMessage,
            "agent.message",
            StoredPolicy::Durable,
            r#"{"m":1}"#,
        )],
    )
    .await;

    let view = store.read_view().await.expect("view");
    let head = view.head().await.expect("head");
    assert_eq!(head.global_sequence, Sequence::new(1).expect("sequence"));

    // 无游标 = initial_sync。
    let initial = view
        .replay(None, ReplayLimit::default())
        .await
        .expect("initial");
    assert_eq!(initial.reset_required, Some(ResetReason::InitialSync));

    // epoch 不同 → epoch_mismatch。
    let mismatch = view
        .replay(
            Some(acp_core::model::GlobalCursor::new(
                acp_core::model::ServerEpoch::new("99999999-9999-4999-8999-999999999999")
                    .expect("epoch"),
                Sequence::new(1).expect("sequence"),
            )),
            ReplayLimit::default(),
        )
        .await
        .expect("mismatch");
    assert_eq!(mismatch.reset_required, Some(ResetReason::EpochMismatch));

    // 超过 head → 协议层的 cursor_invalid（存储层不假装成 reset）。
    let beyond = view
        .replay(
            Some(acp_core::model::GlobalCursor::new(
                head.server_epoch.clone(),
                Sequence::new(99).expect("sequence"),
            )),
            ReplayLimit::default(),
        )
        .await
        .expect_err("beyond head");
    assert!(matches!(
        beyond,
        acp_core::model::PortError::InvalidRequest(_)
    ));

    // 正常续读：next = head 时为 None。
    let caught_up = view
        .replay(Some(head.clone()), ReplayLimit::default())
        .await
        .expect("caught up");
    assert!(caught_up.events.is_empty());
    assert!(caught_up.next.is_none());
    drop(view);
    store.close().await;
}

/// §9.8：容量顺序（过期 delta → 过期正文/状态 → 已压缩批次）与被删行集合。
#[tokio::test]
async fn capacity_cleanup_follows_the_documented_order() {
    let dir = temp_dir("retention-capacity");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at("2026-09-18T00:00:00.000Z"))
        .await
        .expect("open");
    let session = create(&store, 0).await;
    let padding = "y".repeat(240);
    append(
        &store,
        &session,
        1,
        vec![
            event(
                EventKind::Delta,
                "agent.message",
                StoredPolicy::ShortTerm,
                &format!(r#"{{"d":"{padding}"}}"#),
            ),
            event(
                EventKind::FinalMessage,
                "agent.message",
                StoredPolicy::Durable,
                &format!(r#"{{"m":"{padding}"}}"#),
            ),
        ],
    )
    .await;

    // 先把 delta 变成「已过期」，再把正文变成「已压缩批次」。
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    // 必须相对**提交时刻**（02:00）真的过期，否则容量清理不会碰它。
    sqlx::query("UPDATE owned_event SET expires_at = ?1 WHERE kind = 'delta'")
        .bind("2026-09-18T01:30:00.000Z")
        .execute(&pool)
        .await
        .expect("expire delta");
    sqlx::query("UPDATE owned_event SET compacted_into = 1 WHERE kind = 'final_message'")
        .execute(&pool)
        .await
        .expect("mark compacted");
    pool.close().await;

    let pool = raw_pool(&path).await;
    let events_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(length(payload_json)), 0) + COALESCE(SUM(length(acp_raw_json)), 0) \
         FROM owned_event",
    )
    .fetch_one(&pool)
    .await
    .expect("bytes");
    pool.close().await;
    let second_view = format!(r#"{{"small":"{}"}}"#, "q".repeat(340));
    // 派生预算：实测总量 + 半条事件 → 本次 append 是把它顶到上限之上的那一下。
    store.close().await;
    let measured_pool = raw_pool(&path).await;
    let measured = measured_storage_bytes(&measured_pool).await;
    measured_pool.close().await;
    let mut config = StorageConfig::new(&dir);
    config.max_total_size_bytes = measured + second_view.len() as u64 / 2;
    let store = SqliteStore::open(config, &at("2026-09-18T02:00:00.000Z"))
        .await
        .expect("reopen");
    assert!(
        events_bytes + second_view.len() as i64 > 0,
        "precondition: the append must add {} B",
        second_view.len()
    );

    // 触发容量清理：过期 delta 先走，本次写入不再超限（① 足够，③ 不该被动用）。
    append(
        &store,
        &session,
        2,
        vec![event(
            EventKind::Delta,
            "agent.message",
            StoredPolicy::ShortTerm,
            &second_view,
        )],
    )
    .await;

    let pool = raw_pool(&path).await;
    let expired_deltas: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM owned_event WHERE kind = 'delta' AND expires_at < ?1",
    )
    .bind("2026-09-18T02:00:00.000Z")
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(
        expired_deltas, 0,
        "the expired delta must be the first thing removed"
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_event WHERE compacted_into IS NOT NULL"
        )
        .await,
        1,
        "compacted rows are only removed after expired ones"
    );
    pool.close().await;

    // 清掉所有可清理行仍超限 → `prune` 报告 `still_over_limit`（容量压力交给调用方拒绝写入）。
    let tiny = RetentionPolicy {
        max_total_size_bytes: 1,
        ..policy()
    };
    let report: PruneReport = store
        .prune(tiny, at("2026-09-20T00:00:00.000Z"))
        .await
        .expect("prune");
    assert!(report.still_over_limit);
    store.close().await;
}

/// §9.8：任何清理都救不了的时候，`commit` 返回 `Unavailable(StorageFull)` 并且不落任何行。
#[tokio::test]
async fn commit_refuses_to_write_when_capacity_cannot_be_recovered() {
    let dir = temp_dir("retention-full");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at("2026-09-18T00:00:00.000Z"))
        .await
        .expect("open");
    let session = create(&store, 0).await;

    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_pool(&path).await;
    let before = table_snapshot(&pool, "owned_event").await;
    pool.close().await;

    // 派生预算：实测总量 + 半条事件 → 下面那条 1 KiB 的正文必然超限且无从清理。
    store.close().await;
    let measured_pool = raw_pool(&path).await;
    let measured = measured_storage_bytes(&measured_pool).await;
    measured_pool.close().await;
    let mut config = StorageConfig::new(&dir);
    config.max_total_size_bytes = measured + 512;
    let store = SqliteStore::open(config, &at("2026-09-18T01:00:00.000Z"))
        .await
        .expect("reopen");

    let padding = "z".repeat(1024);
    let error = store
        .commit(OwnedCommit {
            session: Some(session),
            at: at("2026-09-18T01:00:00.000Z"),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(
                EventKind::FinalMessage,
                "agent.message",
                StoredPolicy::Durable,
                &format!(r#"{{"m":"{padding}"}}"#),
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect_err("must refuse");
    assert!(matches!(
        error,
        acp_core::model::PortError::Unavailable(acp_core::model::UnavailableKind::StorageFull)
    ));

    let pool = raw_pool(&path).await;
    assert_eq!(table_snapshot(&pool, "owned_event").await, before);
    pool.close().await;
    store.close().await;
}

/// 单会话上限只作用于该会话（§7.5 的 `max_session_size_bytes`）。
#[tokio::test]
async fn session_limit_is_enforced_per_session() {
    let dir = temp_dir("retention-session-limit");
    let mut config = StorageConfig::new(&dir);
    config.max_session_size_bytes = 512;
    let store = SqliteStore::open(config, &at("2026-09-18T00:00:00.000Z"))
        .await
        .expect("open");
    let session = create(&store, 0).await;
    let padding = "w".repeat(1024);
    let error = store
        .commit(OwnedCommit {
            session: Some(session),
            at: at("2026-09-18T01:00:00.000Z"),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(
                EventKind::FinalMessage,
                "agent.message",
                StoredPolicy::Durable,
                &format!(r#"{{"m":"{padding}"}}"#),
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect_err("session limit");
    assert!(matches!(
        error,
        acp_core::model::PortError::Unavailable(acp_core::model::UnavailableKind::StorageFull)
    ));
    store.close().await;
}

/// §7.5 ②′ 回归：一条**已过期但仍被 `owned_interaction.request_event` 引用**的交互事件，不能让
/// `prune` 撞外键（旧实现会返回 `Backend`(FOREIGN KEY constraint failed) 并把整个事务拖回滚）。
#[tokio::test]
async fn prune_clears_expired_events_referenced_by_terminal_interactions() {
    use acp_core::model::{
        InteractionId, InteractionKind, InteractionOption, InteractionResolution,
        PendingInteraction, PermissionDecision, PermissionDecisionKind,
    };
    use acp_core::ports::{
        InteractionResolved, ModeChange, PendingInteractionWrite, SessionUpdate,
    };

    const INTERACTION: &str = "44444444-4444-4444-8444-444444444444";
    let dir = temp_dir("retention-fk-interaction");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at("2026-09-18T00:00:00.000Z"))
        .await
        .expect("open");
    let session = create(&store, 0).await;

    // 交互事件（view 内带 interactionId，kind = interaction）+ 对应的 pending 行，同一事务。
    let view = format!(r#"{{"interactionId":"{INTERACTION}","options":[]}}"#);
    let interaction = PendingInteraction::try_new(
        InteractionId::new(INTERACTION).expect("interaction id"),
        InteractionKind::Permission,
        session.clone(),
        at("2026-09-18T01:00:00.000Z"),
        vec![InteractionOption::try_new("allow-once", "Allow once", "allow_once").expect("option")],
    )
    .expect("pending interaction");
    let written = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at("2026-09-18T01:00:00.000Z"),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(
                EventKind::Interaction,
                "session.request_permission",
                StoredPolicy::Durable,
                &view,
            )],
            interactions: vec![PendingInteractionWrite {
                interaction,
                turn: None,
            }],
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("interaction event + pending row");
    let _interaction_event = written.appended[0].id.clone();

    // 解析它 → 交互行进入终态；再加一条同样过期的 delta，用来证明 ① 照常发生。
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at("2026-09-18T01:00:00.000Z"),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: Some(InteractionResolved {
                    interaction: InteractionId::new(INTERACTION).expect("interaction id"),
                    resolution: InteractionResolution::Permission(
                        PermissionDecision::try_new(
                            "allow-once",
                            PermissionDecisionKind::AllowOnce,
                        )
                        .expect("decision"),
                    ),
                    resolved_by: acp_core::model::Actor::LocalCli,
                }),
            })),
            turns: Vec::new(),
            events: vec![event(
                EventKind::Delta,
                "agent.message",
                StoredPolicy::ShortTerm,
                r#"{"d":1}"#,
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("resolve interaction");

    // 两条事件都过期（相对 prune 时刻）。
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    sqlx::query("UPDATE owned_event SET expires_at = '2026-09-19T00:00:00.000Z'")
        .execute(&pool)
        .await
        .expect("expire events");
    pool.close().await;

    let report = store
        .prune(policy(), at("2026-09-20T00:00:00.000Z"))
        .await
        .expect("prune must not fail on the interaction foreign key");
    assert_eq!(
        report.removed_interactions, 1,
        "②′ removes the terminal row"
    );
    assert_eq!(report.removed_events, 2, "then ①/② can remove both events");

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_event").await,
        0
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_interaction").await,
        0
    );
    // 悬空引用为 0（§9.10）。
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_interaction i WHERE NOT EXISTS \
             (SELECT 1 FROM owned_event e WHERE e.global_sequence = i.request_event)",
        )
        .await,
        0
    );
    pool.close().await;
    store.close().await;
}

/// §7.5 ⑥ 回归：容量超限但清理受阻时，`commit` 必须给出 `Unavailable(StorageFull)`，
/// **不是** `PortError::Backend`（外键错误）。
#[tokio::test]
async fn capacity_reports_storage_full_not_backend_when_events_are_referenced() {
    use acp_core::model::{
        InteractionId, InteractionKind, InteractionResolution, PendingInteraction,
        PermissionDecision, PermissionDecisionKind,
    };
    use acp_core::ports::{
        InteractionResolved, ModeChange, PendingInteractionWrite, SessionUpdate,
    };

    const INTERACTION: &str = "55555555-5555-4555-8555-555555555555";
    let dir = temp_dir("retention-fk-capacity");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at("2026-09-18T00:00:00.000Z"))
        .await
        .expect("open");
    let session = create(&store, 0).await;

    let view = format!(r#"{{"interactionId":"{INTERACTION}","options":[]}}"#);
    let interaction = PendingInteraction::try_new(
        InteractionId::new(INTERACTION).expect("interaction id"),
        InteractionKind::Permission,
        session.clone(),
        at("2026-09-18T01:00:00.000Z"),
        Vec::new(),
    )
    .expect("pending interaction");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at("2026-09-18T01:00:00.000Z"),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(
                EventKind::Interaction,
                "session.request_permission",
                StoredPolicy::Durable,
                &view,
            )],
            interactions: vec![PendingInteractionWrite {
                interaction,
                turn: None,
            }],
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("interaction event + pending row");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at("2026-09-18T01:00:00.000Z"),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: Some(InteractionResolved {
                    interaction: InteractionId::new(INTERACTION).expect("interaction id"),
                    resolution: InteractionResolution::Permission(
                        PermissionDecision::try_new(
                            "allow-once",
                            PermissionDecisionKind::AllowOnce,
                        )
                        .expect("decision"),
                    ),
                    resolved_by: acp_core::model::Actor::LocalCli,
                }),
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
        .expect("resolve");

    // 交互事件过期；再压入一条**不会**被清理的大正文（Durable、未过期）把库顶到上限之上。
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    sqlx::query("UPDATE owned_event SET expires_at = '2026-09-19T00:00:00.000Z'")
        .execute(&pool)
        .await
        .expect("expire");
    pool.close().await;

    // 派生预算：实测总量 + 半条事件 → 1 KiB 正文超限，且 ②′/①② 清完之后仍超限。
    store.close().await;
    let measured_pool = raw_pool(&path).await;
    let measured = measured_storage_bytes(&measured_pool).await;
    measured_pool.close().await;
    let mut config = StorageConfig::new(&dir);
    config.max_total_size_bytes = measured + 512;
    let store = SqliteStore::open(config, &at("2026-09-18T02:00:00.000Z"))
        .await
        .expect("reopen");

    let padding = "z".repeat(1024);
    let error = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at("2026-09-18T02:00:00.000Z"),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(
                EventKind::FinalMessage,
                "agent.message",
                StoredPolicy::Durable,
                &format!(r#"{{"m":"{padding}"}}"#),
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect_err("capacity must reject the write");
    assert!(
        matches!(
            error,
            acp_core::model::PortError::Unavailable(acp_core::model::UnavailableKind::StorageFull)
        ),
        "must be StorageFull, not a foreign-key Backend error: {error}"
    );
    store.close().await;
}

/// §7.5 的度量只能有一个口径：存储层（`open` 时从 schema 现读）与测试助手（同一算法）必须给出同一个数。
/// 这条断言是防「两处度量各自漂移」的门禁——v1 早期版本就因为两处口径不同而误诊过一次。
#[tokio::test]
async fn capacity_measure_matches_the_store() {
    let dir = temp_dir("retention-measure");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at("2026-09-18T00:00:00.000Z"))
        .await
        .expect("open");
    let session = create(&store, 0).await;
    let padding = "m".repeat(200);
    append(
        &store,
        &session,
        1,
        vec![
            event(
                EventKind::FinalMessage,
                "agent.message",
                StoredPolicy::Durable,
                &format!(r#"{{"t":"{padding}"}}"#),
            ),
            event(
                EventKind::Delta,
                "agent.message",
                StoredPolicy::ShortTerm,
                &format!(r#"{{"d":"{padding}"}}"#),
            ),
        ],
    )
    .await;

    let health_bytes = store.health().await.expect("health").total_bytes;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_pool(&path).await;
    let helper_bytes = measured_storage_bytes(&pool).await;
    pool.close().await;

    assert_eq!(
        health_bytes, helper_bytes,
        "存储层与测试助手的度量必须同口径"
    );
    assert!(health_bytes > 0, "度量必须覆盖已写入的正文");
    store.close().await;
}
