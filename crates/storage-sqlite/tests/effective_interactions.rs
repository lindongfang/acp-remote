//! §3.3/§5.8/§6 第 13 条/§7.6：有效待处理读取、版本校验、原子胜出与失效不可复活。

mod support;

use acp_core::model::{
    Actor, AgentId, AgentRef, DeviceId, ElicitationAction, ElicitationContentValue,
    ElicitationValues, EntityRef, EventKind, EventOrigin, EventPayload, EventType, InteractionId,
    InteractionKind, InteractionOption, InteractionResolution, OriginEpoch, PendingInteraction,
    PermissionDecision, PermissionDecisionKind, PortError, Resolution, SessionId, StoredPolicy,
    Timestamp, Version, ViewJson,
};
use acp_core::ports::{
    EffectiveResolution, HistoryInclude, HistoryQuery, InteractionAuthority, InteractionResolved,
    ModeChange, NewSession, OwnedCommit, PendingInteractionWrite, ReplayLimit, SessionStore,
    SessionUpdate, StateChange,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
const DEVICE: &str = "55555555-5555-4555-8555-555555555555";
const OTHER_DEVICE: &str = "66666666-6666-4666-8666-666666666666";
const PERMISSION: &str = "44444444-4444-4444-8444-444444444444";
const ELICITATION: &str = "44444444-4444-4444-8444-444444444445";

fn at(hour: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{hour:02}:00:00.000Z")).expect("timestamp")
}

fn epoch() -> OriginEpoch {
    OriginEpoch::new(EPOCH).expect("epoch")
}

async fn open_store(dir: &std::path::Path) -> SqliteStore {
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
                title: Some("interactions".to_owned()),
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
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id")
}

fn event(event_type: &str, view: &str) -> acp_core::model::PendingEvent {
    acp_core::model::PendingEvent::new(
        EventKind::Interaction,
        EventType::new(event_type).expect("event type"),
        StoredPolicy::Durable,
        EventPayload::new(ViewJson::new(view).expect("view"), None),
        EventOrigin::Agent,
        None,
        None,
    )
}

async fn create_permission(
    store: &SqliteStore,
    session: &SessionId,
    interaction: &str,
) -> InteractionId {
    let id = InteractionId::new(interaction).expect("interaction id");
    let options = vec![
        InteractionOption::try_new("allow-once", "Allow once", "allow_once").expect("option"),
        InteractionOption::try_new("reject-once", "Reject once", "reject_once").expect("option"),
    ];
    let view = format!(
        r#"{{"interactionId":"{interaction}","options":[{{"optionId":"allow-once","label":"Allow once","kind":"allow_once"}},{{"optionId":"reject-once","label":"Reject once","kind":"reject_once"}}]}}"#
    );
    let pending = PendingInteraction::try_new(
        id.clone(),
        InteractionKind::Permission,
        session.clone(),
        at(1),
        options,
    )
    .expect("pending");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event("session.request_permission", &view)],
            interactions: vec![PendingInteractionWrite {
                interaction: pending,
                turn: None,
            }],
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("create permission");
    id
}

async fn create_elicitation(store: &SqliteStore, session: &SessionId) -> InteractionId {
    let id = InteractionId::new(ELICITATION).expect("interaction id");
    let view = format!(
        r#"{{"interactionId":"{ELICITATION}","schema":{{"type":"object","required":["count","mode"],"properties":{{"count":{{"type":"integer","minimum":1,"maximum":10}},"mode":{{"type":"string","enum":["fast","safe"]}},"note":{{"type":"string","maxLength":5}}}}}}}}"#
    );
    let pending = PendingInteraction::try_new(
        id.clone(),
        InteractionKind::Elicitation,
        session.clone(),
        at(1),
        Vec::new(),
    )
    .expect("pending");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event("session.request_elicitation", &view)],
            interactions: vec![PendingInteractionWrite {
                interaction: pending,
                turn: None,
            }],
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("create elicitation");
    id
}

fn actor(device: &str) -> Actor {
    Actor::Device {
        device: DeviceId::new(device).expect("device"),
    }
}

fn permission(option: &str) -> InteractionResolution {
    InteractionResolution::Permission(
        PermissionDecision::try_new(option, PermissionDecisionKind::AllowOnce).expect("decision"),
    )
}

fn elicitation(values: &str) -> InteractionResolution {
    InteractionResolution::Elicitation {
        action: ElicitationAction::Submit,
        values: ElicitationValues::parse_json(values).expect("values"),
    }
}

fn resolution(
    interaction: &InteractionId,
    session: &SessionId,
    request_version: Option<Version>,
    resolution: InteractionResolution,
) -> EffectiveResolution {
    EffectiveResolution {
        interaction: interaction.clone(),
        session: session.clone(),
        actor: actor(OTHER_DEVICE),
        request_version,
        resolution,
        at: at(2),
    }
}

/// 有效读取给出完整请求（选项/表单来自创建事件）、版本与会话；无上游字段时不被伪造。
#[tokio::test]
async fn effective_returns_the_full_current_request() {
    let dir = temp_dir("interactions-effective");
    let store = open_store(&dir).await;
    let session = create_session(&store).await;
    let permission_id = create_permission(&store, &session, PERMISSION).await;
    let elicitation = create_elicitation(&store, &session).await;

    let effective = store.effective(&session).await.expect("effective");
    assert_eq!(effective.len(), 2);
    let permission_entry = effective
        .iter()
        .find(|entry| entry.interaction == permission_id)
        .expect("permission");
    assert_eq!(permission_entry.kind, InteractionKind::Permission);
    assert_eq!(permission_entry.request_version, Version::new(1));
    assert_eq!(permission_entry.options.len(), 2);
    assert_eq!(permission_entry.schema, None);

    let elicitation_entry = effective
        .iter()
        .find(|entry| entry.interaction == elicitation)
        .expect("elicitation");
    assert_eq!(elicitation_entry.kind, InteractionKind::Elicitation);
    assert!(elicitation_entry.options.is_empty());
    let schema = elicitation_entry.schema.as_ref().expect("schema");
    assert!(
        schema
            .as_json()
            .contains("\"required\":[\"count\",\"mode\"]")
    );
    assert_eq!(elicitation_entry.request_version, Version::new(1));

    // 表单校验：合法值通过；缺必填/超范围/错枚举/未知类型都失败（不消费请求）。
    let valid = ElicitationValues::from_fields([
        ("count".to_owned(), ElicitationContentValue::Integer(5)),
        (
            "mode".to_owned(),
            ElicitationContentValue::Text("fast".to_owned()),
        ),
    ])
    .expect("values");
    elicitation_entry
        .validate_submission(&valid)
        .expect("valid submission");
    for bad in [
        r#"{"count":5}"#,
        r#"{"count":0,"mode":"fast"}"#,
        r#"{"count":11,"mode":"fast"}"#,
        r#"{"count":5,"mode":"slow"}"#,
        r#"{"count":5,"mode":"fast","note":"toolong"}"#,
        r#"{"count":5,"mode":"fast","unknown":true}"#,
    ] {
        let values = ElicitationValues::parse_json(bad).expect("values parse");
        assert!(
            elicitation_entry.validate_submission(&values).is_err(),
            "invalid values must be rejected: {bad}"
        );
    }
    // 未知类型（object）对必填字段一律拒绝。
    let bad_schema = acp_core::model::ElicitationSchema::new(
        r#"{"type":"object","required":["x"],"properties":{"x":{"type":"object"}}}"#,
    )
    .expect("schema");
    let object_value = ElicitationValues::from_fields([(
        "x".to_owned(),
        ElicitationContentValue::unknown(r#"{"nested":true}"#).expect("unknown"),
    )])
    .expect("values");
    assert!(bad_schema.validate_values(&object_value).is_err());

    store.close().await;
}

/// 版本不符不消费请求；首个有效答案原子胜出；重复/后到答案得到已处理事实。
#[tokio::test]
async fn version_checks_and_first_writer_win() {
    let dir = temp_dir("interactions-race");
    let store = open_store(&dir).await;
    let session = create_session(&store).await;
    let interaction = create_permission(&store, &session, PERMISSION).await;

    // 版本不符 → VersionMismatch，且请求仍 pending。
    let error = store
        .resolve_effective(resolution(
            &interaction,
            &session,
            Some(Version::new(2)),
            permission("allow-once"),
        ))
        .await
        .expect_err("stale version");
    assert!(matches!(
        error,
        PortError::Conflict(acp_core::model::ConflictKind::VersionMismatch)
    ));
    assert_eq!(
        store.effective(&session).await.expect("effective").len(),
        1,
        "版本校验失败不得消费请求"
    );

    // 未知交互 / 跨会话 → NotFound（不泄露其它会话的交互事实）。
    let unknown = InteractionId::new("44444444-4444-4444-8444-999999999999").expect("id");
    let error = store
        .resolve_effective(resolution(
            &unknown,
            &session,
            None,
            permission("allow-once"),
        ))
        .await
        .expect_err("unknown");
    assert!(matches!(
        error,
        PortError::NotFound(EntityRef::Interaction(_))
    ));
    let other_session = SessionId::new("77777777-7777-4777-8777-777777777777").expect("session id");
    let error = store
        .resolve_effective(resolution(
            &interaction,
            &other_session,
            None,
            permission("allow-once"),
        ))
        .await
        .expect_err("cross-session");
    assert!(matches!(
        error,
        PortError::NotFound(EntityRef::Interaction(_))
    ));

    // 两个设备并发提交：恰好一个 Resolved，另一个 AlreadyResolved。
    let mut first = resolution(
        &interaction,
        &session,
        Some(Version::new(1)),
        permission("allow-once"),
    );
    first.actor = actor(DEVICE);
    let second = resolution(
        &interaction,
        &session,
        Some(Version::new(1)),
        permission("reject-once"),
    );
    let (left, right) = tokio::join!(
        store.resolve_effective(first),
        store.resolve_effective(second),
    );
    let outcomes = [left.expect("first"), right.expect("second")];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| **outcome == Resolution::Resolved)
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| **outcome == Resolution::AlreadyResolved)
            .count(),
        1
    );
    assert!(
        store
            .effective(&session)
            .await
            .expect("effective")
            .is_empty()
    );

    // 重复同一 requestId 的语义由用例层幂等处理；存储层对已处理请求继续返回事实。
    assert_eq!(
        store
            .resolve_effective(resolution(
                &interaction,
                &session,
                None,
                permission("allow-once"),
            ))
            .await
            .expect("again"),
        Resolution::AlreadyResolved
    );

    store.close().await;
}

/// 失效标记：不再出现在有效读取里、不可复活；重启恢复后可批量失效。
#[tokio::test]
async fn invalidated_requests_cannot_be_revived() {
    let dir = temp_dir("interactions-invalidate");
    let store = open_store(&dir).await;
    let session = create_session(&store).await;
    let permission_id = create_permission(&store, &session, PERMISSION).await;
    create_elicitation(&store, &session).await;

    assert_eq!(
        store
            .invalidate_pending(Some(&session), at(3))
            .await
            .expect("invalidate"),
        2
    );
    assert_eq!(
        store
            .invalidate_pending(Some(&session), at(3))
            .await
            .expect("idempotent"),
        0
    );
    assert!(
        store
            .effective(&session)
            .await
            .expect("effective")
            .is_empty()
    );
    let error = store
        .resolve_effective(resolution(
            &permission_id,
            &session,
            None,
            permission("allow-once"),
        ))
        .await
        .expect_err("invalidated");
    assert!(matches!(
        error,
        PortError::Conflict(acp_core::model::ConflictKind::Expired)
    ));

    // 重开（Daemon 重启）不会把失效行重新标成 pending；再次批量失效仍为 0。
    store.close().await;
    let reopened = open_store(&dir).await;
    assert!(
        reopened
            .effective(&session)
            .await
            .expect("effective after reopen")
            .is_empty()
    );
    assert_eq!(
        reopened
            .invalidate_pending(None, at(4))
            .await
            .expect("sweep"),
        0
    );
    let error = reopened
        .resolve_effective(resolution(
            &permission_id,
            &session,
            None,
            permission("allow-once"),
        ))
        .await
        .expect_err("still invalidated");
    assert!(matches!(
        error,
        PortError::Conflict(acp_core::model::ConflictKind::Expired)
    ));
    reopened.close().await;
}

/// 失效后：旧 pending 读面（`read_session`、Node Link 快照）不再返回它，旧解析路径也不能消费它。
#[tokio::test]
async fn invalidated_requests_disappear_from_every_pending_reader() {
    let dir = temp_dir("interactions-invalidate-readers");
    let store = open_store(&dir).await;
    let session = create_session(&store).await;
    let permission_id = create_permission(&store, &session, PERMISSION).await;
    create_elicitation(&store, &session).await;
    assert_eq!(
        store
            .invalidate_pending(Some(&session), at(3))
            .await
            .expect("invalidate"),
        2
    );

    // `session.read` 的 `pending_interactions` 不再包含它。
    let view = store.read_view().await.expect("view");
    let page = view
        .read_session(HistoryQuery {
            session: session.clone(),
            include: HistoryInclude {
                messages: false,
                turns: false,
                pending_interactions: true,
                config_options: false,
                capabilities: false,
                history: false,
            },
            before_history: None,
            before_messages: None,
            messages_limit: ReplayLimit::new(0),
            after: None,
            limit: ReplayLimit::new(64),
        })
        .await
        .expect("read");
    assert!(
        page.interactions.is_empty(),
        "失效请求不得继续出现在历史读里"
    );

    // Node Link 快照的 `pending_interactions` 同样为空。
    let slice = view
        .node_link_slice(&session, None, ReplayLimit::new(64))
        .await
        .expect("node link slice");
    assert!(
        slice.pending_interactions.is_empty(),
        "失效请求不得继续进入 Node Link 快照"
    );

    // 旧解析路径（`StateChange::Update.interaction`）拒绝消费，并把原因说成 Expired。
    let commit = OwnedCommit {
        session: Some(session.clone()),
        at: at(4),
        expected_version: None,
        state: Some(StateChange::Update(SessionUpdate {
            state: None,
            mode: ModeChange::Unchanged,
            title: None,
            closed_at: None,
            interaction: Some(InteractionResolved {
                interaction: permission_id.clone(),
                resolution: permission("allow-once"),
                resolved_by: actor(DEVICE),
            }),
            agent_session_id: None,
            workspace_cwd: None,
            workspace_directory_id: None,
        })),
        turns: Vec::new(),
        events: Vec::new(),
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: None,
        command_terminal: None,
        origin_epoch: None,
    };
    let error = store.commit(commit).await.expect_err("expired request");
    assert!(
        matches!(
            error,
            PortError::Conflict(acp_core::model::ConflictKind::Expired)
        ),
        "{error:?}"
    );

    // 行仍是 `expired`（没有被旧路径改写成 resolved），因此之后也不再出现在任何 pending 读面。
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let state: String =
        sqlx::query_scalar("SELECT state FROM owned_interaction WHERE interaction_id = ?1")
            .bind(permission_id.as_str())
            .fetch_one(&pool)
            .await
            .expect("state");
    pool.close().await;
    assert_eq!(state, "expired");
    assert!(
        store
            .effective(&session)
            .await
            .expect("effective")
            .is_empty()
    );
    drop(view);
    store.close().await;
}

/// 表单提交的合法值（精确整数）原样落库，不出现浮点往返。
#[tokio::test]
async fn elicitation_values_round_trip_exactly() {
    let dir = temp_dir("interactions-values");
    let store = open_store(&dir).await;
    let session = create_session(&store).await;
    let interaction = create_elicitation(&store, &session).await;

    let resolution = resolution(
        &interaction,
        &session,
        Some(Version::new(1)),
        elicitation(r#"{"count":9007199254740993,"mode":"safe"}"#),
    );
    store
        .resolve_effective(resolution)
        .await
        .expect("resolve elicitation");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let stored: String = sqlx::query_scalar(
        "SELECT elicitation_values_json FROM owned_interaction WHERE interaction_id = ?1",
    )
    .bind(interaction.as_str())
    .fetch_one(&pool)
    .await
    .expect("values");
    assert_eq!(
        stored, r#"{"count":9007199254740993,"mode":"safe"}"#,
        "整数不得先经 f64"
    );
    let action: String = sqlx::query_scalar(
        "SELECT elicitation_action FROM owned_interaction WHERE interaction_id = ?1",
    )
    .bind(interaction.as_str())
    .fetch_one(&pool)
    .await
    .expect("action");
    assert_eq!(action, "submit");
    pool.close().await;

    store.close().await;
}
