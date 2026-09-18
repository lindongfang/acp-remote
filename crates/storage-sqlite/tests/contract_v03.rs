//! 合同 v0.3 的新判据：§9.15 交互创建、§9.16 正文读取、§9.17 非会话级事件的三列 NULL。

mod support;

use acp_core::model::{
    AcpRaw, AgentId, AgentRef, Digest, EventId, EventKind, EventOrigin, EventPayload, EventType,
    InteractionId, InteractionKind, InteractionOption, OriginEpoch, PendingEvent,
    PendingInteraction, PortError, RawUnavailableReason, Sequence, SessionId, StoredPolicy,
    Timestamp, ViewJson,
};
use acp_core::ports::{
    NewSession, OwnedCommit, PendingInteractionWrite, ReplayLimit, SessionStore, StateChange,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
const INTERACTION: &str = "44444444-4444-4444-8444-444444444444";

/// 带嵌套与未知字段的视图（用于验证「原样文本、不得重新序列化」）。
///
/// 必须是 **ACPR-CJ1 规范形**（成员名升序、整数在 |n| <= 2^53-1 内、控制字符用小写 `\u00xx`）：
/// §7.3/§9.9 要求 `payload_digest = SHA256(ACPR-CJ1(payload_json))`，非规范 view 会在提交时被拒。
const RICH_VIEW: &str = r#"{"nested":{"a":"  spaced  ","b":[1,2,{"c":null}]},"unicode":"привет\u0000","unknown":{"big":1234567890},"z":1}"#;
/// 带非标准空白的 ACP 原文（同样必须逐字节保真）。
const RICH_ACP: &str = r#"{ "tool" : "read" ,"args" : [ 1 , 2 ] }"#;

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

fn epoch() -> OriginEpoch {
    OriginEpoch::new(EPOCH).expect("epoch")
}

fn digest(seed: &str) -> Digest {
    Digest::new(&support::digest_text(seed)).expect("digest")
}

fn payload(view: &str, acp: Option<AcpRaw>) -> EventPayload {
    EventPayload::new(ViewJson::new(view).expect("view"), acp)
}

fn pending(kind: EventKind, event_type: &str, payload: EventPayload) -> PendingEvent {
    PendingEvent::new(
        kind,
        EventType::new(event_type).expect("event type"),
        StoredPolicy::Durable,
        payload,
        EventOrigin::Agent,
        None,
        None,
    )
}

async fn store(dir: &std::path::Path) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at(0))
        .await
        .expect("open store")
}

async fn create_session(store: &SqliteStore) -> SessionId {
    store
        .commit(OwnedCommit {
            session: None,
            at: at(0),
            expected_version: None,
            state: Some(StateChange::Create(NewSession {
                title: Some("v03".to_owned()),
                agent: agent(),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id")
}

/// §9.15：`owned_interaction` 恰好一行且 `request_event` 指向交互事件；`pending_interactions` 读回的
/// 行 `options` 为空，而 `event_payload(request_event)` 能还原出正文里的候选项。
#[tokio::test]
async fn interaction_creation_links_the_request_event() {
    let dir = temp_dir("v03-interaction-create");
    let store = store(&dir).await;
    let session = create_session(&store).await;

    // 单次提交：`interaction` 事件（view 内带 `interactionId`）+ 对应的 `PendingInteractionWrite`。
    // `request_event` 由 broker 填**占位值**（§5.4 的 event_id 由存储层分配，broker 无从得知）。
    let options_view = format!(
        r#"{{"interactionId":"{INTERACTION}","options":[{{"optionId":"allow-once","label":"Allow once","kind":"allow_once"}}],"pad":"opaque-options-marker"}}"#
    );
    let interaction = PendingInteraction::try_new(
        InteractionId::new(INTERACTION).expect("interaction id"),
        InteractionKind::Permission,
        session.clone(),
        at(1),
        vec![InteractionOption::try_new("allow-once", "Allow once", "allow_once").expect("option")],
    )
    .expect("pending interaction");
    let written = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![pending(
                EventKind::Interaction,
                "session.request_permission",
                payload(&options_view, None),
            )],
            interactions: vec![PendingInteractionWrite {
                interaction,
                turn: None,
            }],
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("create pending interaction in one commit");
    let request_event = written.appended[0].id.clone();
    {
        let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
        let stored_id: String = sqlx::query_scalar(
            "SELECT e.event_id FROM owned_interaction i JOIN owned_event e \
             ON e.global_sequence = i.request_event",
        )
        .fetch_one(&pool)
        .await
        .expect("paired event");
        assert_eq!(
            stored_id,
            request_event.as_str(),
            "request_event must point at the paired event"
        );
        pool.close().await;
    }

    // 恰好一行，且 `request_event` 指向该事件的行号（§9.10 的悬空引用为 0）。
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_interaction").await,
        1
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_interaction i JOIN owned_event e \
             ON e.global_sequence = i.request_event",
        )
        .await,
        1,
        "request_event must point at the interaction event row"
    );
    pool.close().await;

    // 读回：`options` 为空（§7.3 的列集合里没有 options），正文经 `event_payload` 还原。
    let view = store.read_view().await.expect("read view");
    let page = view
        .read_session(acp_core::ports::HistoryQuery {
            session: session.clone(),
            include: acp_core::ports::HistoryInclude {
                messages: false,
                turns: false,
                pending_interactions: true,
                config_options: false,
                capabilities: false,
            },
            after: None,
            limit: ReplayLimit::default(),
        })
        .await
        .expect("history");
    assert_eq!(page.interactions.len(), 1);
    assert_eq!(page.interactions[0].id().as_str(), INTERACTION);
    assert!(
        page.interactions[0].options().is_empty(),
        "the persisted row carries metadata only"
    );

    let recovered = view
        .event_payload(&request_event)
        .await
        .expect("payload")
        .expect("event exists");
    assert!(
        recovered.view.as_str().contains("opaque-options-marker"),
        "the interaction event payload must be recoverable by request_event"
    );
    drop(view);
    store.close().await;
}

/// §9.15 的另一半：同一提交里同时创建与解析交互 → `InvalidRequest`。
#[tokio::test]
async fn creating_and_resolving_in_one_commit_is_rejected() {
    use acp_core::model::{InteractionResolution, PermissionDecision, PermissionDecisionKind};
    use acp_core::ports::{InteractionResolved, ModeChange, SessionUpdate};

    let dir = temp_dir("v03-interaction-mutex");
    let store = store(&dir).await;
    let session = create_session(&store).await;

    let event = pending(
        EventKind::Interaction,
        "session.request_permission",
        payload(r#"{"options":[]}"#, None),
    );
    let _written = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event],
            interactions: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("event");

    let interaction = PendingInteraction::try_new(
        InteractionId::new(INTERACTION).expect("interaction id"),
        InteractionKind::Permission,
        session.clone(),
        at(1),
        Vec::new(),
    )
    .expect("pending interaction");

    let error = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: Some(InteractionResolved {
                    interaction: InteractionId::new(INTERACTION).expect("id"),
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
            interactions: vec![PendingInteractionWrite {
                interaction,
                turn: None,
            }],
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect_err("create + resolve in one commit must be rejected");
    assert!(matches!(error, PortError::InvalidRequest(_)));

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_interaction").await,
        0,
        "a rejected commit must not write the interaction row"
    );
    pool.close().await;
    store.close().await;
}

/// §9.16：`event_payload` 的 `view` 与库内 `payload_json` 逐字节相同，`AcpRaw::Available.raw_json`
/// 逐字节保真，`Unavailable` 行的 `reason` 与列一致，未知 id → `None`。
#[tokio::test]
async fn event_payload_is_byte_exact() {
    let dir = temp_dir("v03-event-payload");
    let store = store(&dir).await;
    let session = create_session(&store).await;
    let sha = digest("acp-raw");

    let available = pending(
        EventKind::Structured,
        "agent.tool_call",
        payload(
            RICH_VIEW,
            Some(AcpRaw::available("application/json", RICH_ACP, sha.clone()).expect("acp")),
        ),
    );
    let unavailable = pending(
        EventKind::Structured,
        "agent.tool_call",
        payload(
            r#"{"note":"raw cleared"}"#,
            Some(AcpRaw::unavailable(
                RawUnavailableReason::RetentionExpired,
                4096,
                Some(sha.clone()),
            )),
        ),
    );
    let written = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![available, unavailable],
            interactions: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("events");
    assert_eq!(written.appended.len(), 2);

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let stored: String =
        sqlx::query_scalar("SELECT payload_json FROM owned_event WHERE event_id = ?1")
            .bind(written.appended[0].id.as_str())
            .fetch_one(&pool)
            .await
            .expect("stored payload");
    assert_eq!(stored, RICH_VIEW, "the view must be stored verbatim");
    pool.close().await;

    let view = store.read_view().await.expect("read view");
    let first = view
        .event_payload(&written.appended[0].id)
        .await
        .expect("payload")
        .expect("event");
    assert_eq!(first.view.as_str(), RICH_VIEW, "byte-identical view");
    let acp = first.acp.expect("acp raw");
    let (media_type, raw_json, byte_length, _sha) = acp.as_available().expect("available");
    assert_eq!(media_type, "application/json");
    assert_eq!(raw_json, RICH_ACP, "byte-identical ACP raw");
    assert_eq!(byte_length, RICH_ACP.len() as u64);

    let second = view
        .event_payload(&written.appended[1].id)
        .await
        .expect("payload")
        .expect("event");
    let restored = second.acp.expect("acp");
    let (reason, byte_length, sha) = restored.as_unavailable().expect("unavailable");
    // §9.16：`reason`/`byte_length`/`sha256` 三者都要与行内列一致 —— 「原文不在、摘要还在」是合法状态。
    assert_eq!(reason, RawUnavailableReason::RetentionExpired);
    assert_eq!(byte_length, 4096);
    assert_eq!(
        sha.map(|value| value.as_str().to_owned()),
        Some(support::digest_text("acp-raw")),
        "the digest must survive the loss of the raw text"
    );
    {
        let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
        let row = sqlx::query(
            "SELECT acp_raw_json, acp_byte_length, acp_sha256, acp_raw_unavailable_reason \
             FROM owned_event WHERE event_id = ?1",
        )
        .bind(written.appended[1].id.as_str())
        .fetch_one(&pool)
        .await
        .expect("unavailable row");
        assert!(
            sqlx::Row::try_get::<Option<String>, _>(&row, "acp_raw_json")
                .expect("raw")
                .is_none()
        );
        assert_eq!(
            sqlx::Row::try_get::<Option<i64>, _>(&row, "acp_byte_length").expect("bytes"),
            Some(4096)
        );
        assert_eq!(
            sqlx::Row::try_get::<Option<String>, _>(&row, "acp_sha256").expect("sha"),
            Some(support::digest_text("acp-raw"))
        );
        assert_eq!(
            sqlx::Row::try_get::<Option<String>, _>(&row, "acp_raw_unavailable_reason")
                .expect("reason"),
            Some("retention_expired".to_owned())
        );
        pool.close().await;
    }

    let unknown = EventId::new("99999999-9999-4999-8999-999999999999").expect("event id");
    assert!(
        view.event_payload(&unknown)
            .await
            .expect("lookup")
            .is_none(),
        "an unknown event id must yield None"
    );
    drop(view);
    store.close().await;
}

/// §9.17：非会话级事件三列 NULL 且仍出现在 `replay`；会话级事件三列非 NULL。
#[tokio::test]
async fn node_scope_events_have_null_origin_columns() {
    let dir = temp_dir("v03-node-scope");
    let store = store(&dir).await;
    let session = create_session(&store).await;

    let node_write = store
        .commit(OwnedCommit {
            session: None,
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![pending(
                EventKind::State,
                "device.revoked",
                payload(
                    r#"{"deviceId":"55555555-5555-4555-8555-555555555555"}"#,
                    None,
                ),
            )],
            interactions: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        })
        .await
        .expect("node-scope event");
    assert!(node_write.appended[0].session.is_none());
    assert!(node_write.appended[0].session_sequence.is_none());
    assert!(node_write.appended[0].origin_epoch.is_none());
    assert!(node_write.appended[0].origin_sequence.is_none());

    let session_write = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(2),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![pending(
                EventKind::FinalMessage,
                "agent.message",
                payload(r#"{"m":1}"#, None),
            )],
            interactions: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("session event");
    assert!(session_write.appended[0].origin_epoch.is_some());
    assert!(session_write.appended[0].origin_sequence.is_some());

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_event WHERE session_id IS NULL \
             AND session_sequence IS NULL AND origin_epoch IS NULL AND origin_sequence IS NULL \
             AND event_type = 'device.revoked'",
        )
        .await,
        1,
        "node-scope rows must have all three origin columns NULL"
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_event WHERE session_id IS NOT NULL \
             AND (session_sequence IS NULL OR origin_epoch IS NULL OR origin_sequence IS NULL)",
        )
        .await,
        0,
        "session-scoped rows must have all three non-NULL"
    );
    pool.close().await;

    let view = store.read_view().await.expect("view");
    let batch = view
        .replay(None, ReplayLimit::default())
        .await
        .expect("replay");
    let node_row = batch
        .events
        .iter()
        .find(|delivery| match delivery {
            acp_core::model::CommittedDelivery::Owned(event) => event.session.is_none(),
            _ => false,
        })
        .expect("the node-scope event must take part in replay");
    match node_row {
        acp_core::model::CommittedDelivery::Owned(event) => {
            assert!(event.origin_epoch.is_none());
            assert!(event.origin_sequence.is_none());
            assert_eq!(event.global_sequence, Sequence::new(1).expect("sequence"));
        }
        _ => unreachable!(),
    }
    drop(view);
    store.close().await;
}

/// §6 第 13 条负例：提交里有 `interactions` 但**没有**配对事件（`kind='interaction'` 且
/// `payload.interactionId` 匹配）→ 整事务拒绝且零写入。
#[tokio::test]
async fn interaction_without_a_paired_event_is_rejected() {
    let dir = temp_dir("v03-interaction-unpaired");
    let store = store(&dir).await;
    let session = create_session(&store).await;

    let interaction = PendingInteraction::try_new(
        InteractionId::new(INTERACTION).expect("interaction id"),
        InteractionKind::Elicitation,
        session.clone(),
        at(1),
        Vec::new(),
    )
    .expect("pending interaction");

    for events in [
        // (a) 完全没有事件
        Vec::new(),
        // (b) 有事件，但 payload 里没有 `interactionId`
        vec![pending(
            EventKind::Interaction,
            "session.request_elicitation",
            payload(r#"{"message":"no id here"}"#, None),
        )],
        // (c) `interactionId` 为 null
        vec![pending(
            EventKind::Interaction,
            "session.request_elicitation",
            payload(r#"{"interactionId":null}"#, None),
        )],
        // (d) `interactionId` 匹配但 kind 不是 `interaction`
        vec![pending(
            EventKind::State,
            "session.state",
            payload(&format!(r#"{{"interactionId":"{INTERACTION}"}}"#), None),
        )],
    ] {
        let error = store
            .commit(OwnedCommit {
                session: Some(session.clone()),
                at: at(1),
                expected_version: None,
                state: None,
                turns: Vec::new(),
                events,
                interactions: vec![PendingInteractionWrite {
                    interaction: interaction.clone(),
                    turn: None,
                }],
                idempotency: None,
                command_terminal: None,
                origin_epoch: Some(epoch()),
            })
            .await
            .expect_err("an unpaired interaction must be rejected");
        assert!(matches!(error, PortError::InvalidRequest(_)));
    }

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_interaction").await,
        0,
        "no interaction row may be written"
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_event").await,
        0,
        "the rejected transaction must also roll back its events"
    );
    pool.close().await;
    store.close().await;
}
