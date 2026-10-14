//! Sync 事件读面（`SYNC_PROTOCOL.md` §10.1 的 `event` 帧；`docs/CORE_PORTS_AND_STORAGE.md` §5.2 的
//! `ReadView::sync_replay`）：定位 + 持久化正文 + `origin.kind`/`origin.deviceId`/`origin.nodeId`/
//! `causationRequestId` 在同一次取行里给出。
//!
//! 覆盖 `1.6` 的完成判据：**五个** `origin_kind` 原样往返；仅 device 类事件的设备 id、仅 node 类事件
//! 的节点 id 非空且等于诱发该事件的命令行 `actor_id`（其余情况恒为 `None`，包括「带 causation 但不属
//! 于本类」这一条）；`causation` 原样（含 `None`）；窗口严格在 `after` 之后、按全局序号升序、受 `limit`
//! 限制、head 正确。
//!
//! 失败关闭（`Corrupt`，不静默降级）：词表外的 `origin_kind`、不是设备 id 的 `actor_id`、不是
//! `node/access-node` 复合键的节点 `actor_id`，以及**历史行无法归因**的三类情形（`causation` 为空 /
//! 对应命令行不在本机 / 命令行的 actor 类别与本事件的 `origin.kind` 不符）——最后一条是 4.8 的显式
//! 要求：升级前写入的 `device` 类事件在本机可能无法归因，读面 MUST NOT 给 `None` 让投递层发出
//! §10.1 禁止的 `{kind:"device", deviceId:null}`。

mod support;

use acp_core::model::{
    Actor, AgentId, AgentRef, CommandKind, DeviceId, Digest, EventKind, EventOrigin, EventPayload,
    EventType, GlobalCursor, NodeId, OriginEpoch, PendingEvent, PortError, RequestId, ScopeSet,
    Sequence, SessionId, StoredPolicy, Timestamp, ViewJson,
};
use acp_core::ports::{
    IdempotencyRecord, NewSession, OwnedCommit, ReplayLimit, SessionStore, StateChange,
};
use storage_sqlite::migrate::{DATABASE_FILE, StorageConfig};
use storage_sqlite::session_store::SqliteStore;
use support::*;

const ORIGIN_EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
const DEVICE: &str = "55555555-5555-4555-8555-555555555555";
const REQUEST: &str = "11111111-1111-4111-8111-111111111111";
const NODE: &str = "66666666-6666-4666-8666-666666666666";
const ACCESS_NODE: &str = "77777777-7777-4777-8777-777777777777";
const NODE_REQUEST: &str = "22222222-2222-4222-8222-222222222222";

fn at(offset_hours: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{offset_hours:02}:00:00.000Z")).expect("timestamp")
}

fn agent() -> AgentRef {
    AgentRef::try_new(
        AgentId::new("probe-agent").expect("agent id"),
        "Probe Agent",
    )
    .expect("agent ref")
}

fn epoch() -> OriginEpoch {
    OriginEpoch::new(ORIGIN_EPOCH).expect("origin epoch")
}

fn device() -> DeviceId {
    DeviceId::new(DEVICE).expect("device id")
}

fn node() -> NodeId {
    NodeId::new(NODE).expect("node id")
}

fn access_node() -> NodeId {
    NodeId::new(ACCESS_NODE).expect("access node id")
}

fn request() -> RequestId {
    RequestId::new(REQUEST).expect("request id")
}

fn node_request() -> RequestId {
    RequestId::new(NODE_REQUEST).expect("request id")
}

/// 一条事件；`view` 固定为带事件类型的探针文档，用来断言「条目带的是它自己那条的正文」。
fn event(event_type: &str, origin: EventOrigin, causation: Option<RequestId>) -> PendingEvent {
    PendingEvent::new(
        EventKind::State,
        EventType::new(event_type).expect("event type"),
        StoredPolicy::Durable,
        EventPayload::new(
            ViewJson::new(&format!("{{\"probe\":\"{event_type}\"}}")).expect("view json"),
            None,
        ),
        origin,
        None,
        causation,
    )
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
                title: Some("sync-replay".to_owned()),
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

/// 一条设备命令行（`actor_kind = 'device'`）。
fn device_command(session: &SessionId, request: RequestId) -> IdempotencyRecord {
    IdempotencyRecord {
        actor: Actor::Device {
            device: device(),
            scopes: ScopeSet::empty(),
        },
        request,
        command: "session.prompt".to_owned(),
        kind: CommandKind::Mutation,
        session: Some(session.clone()),
        expected_version: None,
        request_fingerprint: Digest::new(&digest_text("sync-replay")).expect("digest"),
        accepted_at: at(1),
    }
}

/// 一条节点命令行（`actor_kind = 'node'`；`actor_id` 是 `node/access_node` 复合键）。
fn node_command(session: &SessionId, request: RequestId) -> IdempotencyRecord {
    IdempotencyRecord {
        actor: Actor::Node {
            node: node(),
            access_node: access_node(),
        },
        request,
        command: "session.create".to_owned(),
        kind: CommandKind::Mutation,
        session: Some(session.clone()),
        expected_version: None,
        request_fingerprint: Digest::new(&digest_text("sync-replay-node")).expect("digest"),
        accepted_at: at(2),
    }
}

/// 五条事件覆盖五种 `origin.kind`（顺序 = 落盘顺序 = `global_sequence` 1..5）+ 一条设备命令行
/// + 一条节点命令行（第二次提交带；`owned_command` 的 actor 就是两个身份字段的来源）。
///
/// - ① agent：没有 causation；
/// - ② device：`causation` = 设备命令的 requestId（`deviceId` 的来源）；
/// - ③ daemon：没有 causation；
/// - ④ local_cli：**故意**带与 ② 相同的 causation——它不是设备事件，因此不得回填设备 id；
/// - ⑤ node：`causation` = 节点命令的 requestId（`nodeId` 的来源，取复合键里的 ownerNodeId）。
async fn seed(store: &SqliteStore, session: &SessionId) {
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![
                event("agent.message.delta", EventOrigin::Agent, None),
                event("turn.completed", EventOrigin::Device, Some(request())),
                event("storage.integrity_failed", EventOrigin::Daemon, None),
                event(
                    "session.mode.changed",
                    EventOrigin::LocalCli,
                    Some(request()),
                ),
            ],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(device_command(session, request())),
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("四条事件 + 一条设备命令行");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(2),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![event(
                "session.created",
                EventOrigin::Node,
                Some(node_request()),
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(node_command(session, node_request())),
            command_terminal: None,
            origin_epoch: Some(epoch()),
        })
        .await
        .expect("一条 node 事件 + 一条节点命令行");
}

/// 窗口语义与来源列的原样回传（一次只读事务内的同一页）。
#[tokio::test]
async fn sync_replay_roundtrips_origin_and_window() {
    let dir = temp_dir("sync-replay-window");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    seed(&store, &session).await;

    let view = store.read_view().await.expect("read view");
    let page = view
        .sync_replay(None, ReplayLimit::new(10))
        .await
        .expect("sync replay");

    assert_eq!(page.events.len(), 5, "五条事件都在一页里");
    assert_eq!(
        page.events
            .iter()
            .map(|record| record.origin_kind)
            .collect::<Vec<_>>(),
        vec![
            EventOrigin::Agent,
            EventOrigin::Device,
            EventOrigin::Daemon,
            EventOrigin::LocalCli,
            EventOrigin::Node,
        ],
        "五个 origin_kind 原样往返（顺序即 global_sequence 升序）"
    );
    assert_eq!(
        page.events[1].origin_device_id.as_ref(),
        Some(&device()),
        "device 类事件的 origin.deviceId 等于诱发该事件的命令的 actor_id"
    );
    assert_eq!(
        page.events[4].origin_node_id.as_ref(),
        Some(&node()),
        "node 类事件的 origin.nodeId 等于诱发该事件的命令的主 principal（复合键的 ownerNodeId）"
    );
    assert!(
        page.events[4].origin_device_id.is_none() && page.events[1].origin_node_id.is_none(),
        "两个身份字段互斥（§10.1 的配对规则）"
    );
    assert!(
        page.events.iter().enumerate().all(|(index, record)| {
            match record.origin_kind {
                // device 类只在对应下标上出现，且只带 deviceId。
                EventOrigin::Device => {
                    record.origin_device_id.is_some()
                        && record.origin_node_id.is_none()
                        && index == 1
                }
                // node 类只在对应下标上出现，且只带 nodeId。
                EventOrigin::Node => {
                    record.origin_node_id.is_some()
                        && record.origin_device_id.is_none()
                        && index == 4
                }
                // 其余来源（含带同一个 causation 的 local_cli）两个身份字段都恒为 None。
                _ => record.origin_device_id.is_none() && record.origin_node_id.is_none(),
            }
        }),
        "两个身份字段只在对应 kind 上非空；其余来源（含带同一 causation 的 local_cli）恒为 None"
    );
    assert_eq!(
        page.events[0].causation, None,
        "没有命令直接导致的事件 causationRequestId 为 null"
    );
    assert_eq!(
        page.events[1].causation.as_ref(),
        Some(&request()),
        "causation 原样回传"
    );
    assert_eq!(
        page.events[4].causation.as_ref(),
        Some(&node_request()),
        "node 事件的 causation 也是它自己那条命令的 requestId"
    );
    assert_eq!(
        page.events[0].payload.view.as_str(),
        "{\"probe\":\"agent.message.delta\"}",
        "正文直接取库内 payload_json 的原样文本（不重新序列化）"
    );
    assert_eq!(
        page.head.global_sequence,
        Sequence::new(5).expect("sequence"),
        "head 是这批数据的水位"
    );
    assert_eq!(
        page.events
            .iter()
            .map(|record| record.event.global_sequence.get())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5],
        "按全局序号升序"
    );

    // 严格在 `after` 之后。
    let resumed = view
        .sync_replay(
            Some(GlobalCursor::new(
                page.head.server_epoch.clone(),
                Sequence::new(2).expect("sequence"),
            )),
            ReplayLimit::new(10),
        )
        .await
        .expect("续读");
    assert_eq!(
        resumed
            .events
            .iter()
            .map(|record| record.event.global_sequence.get())
            .collect::<Vec<_>>(),
        vec![3, 4, 5],
        "after 提到的序号本身不重发"
    );

    // `limit` 生效，且页边界停在 limit 条（不多取、不倒序）。
    let limited = view
        .sync_replay(None, ReplayLimit::new(2))
        .await
        .expect("限长");
    assert_eq!(limited.events.len(), 2, "limit 生效");
    assert_eq!(limited.events[1].event.global_sequence.get(), 2);

    // 已到 head：`after` = 当前 head 时是空页（不是错误）。
    let caught_up = view
        .sync_replay(Some(page.head.clone()), ReplayLimit::new(10))
        .await
        .expect("已到 head");
    assert!(caught_up.events.is_empty());
}

/// 历史行失败关闭（§10.1 的配对规则 + 4.8 的处置）：`origin_kind = device`/`node` 而得不出**唯一**
/// 身份时，读面返回 `Corrupt` 而不是 `None`——`{kind:"device", deviceId:null}` 是 §10.1 禁止发送方
/// 产生的形状，而 schema 侧它又合法，因此读面只有显式失败关闭这一条出路。
///
/// 三类情形在真实库里的产生路径各不相同：
///
/// ① **`causation` 为空**：1.7 之前 agent-host 产出的每条 `EndpointEvent` 都不带 `causation`
///    （适配器唯一的生产构造点固定 `None`），broker 原样落盘，于是升级前写入的设备驱动事件全部落在
///    这一类——这是**主流历史形态**；
/// ② **对应命令行不在本机**：`owned_event` 与 `owned_command` 的写入本来就允许「事件在本节点、命令
///    由别处发起」（跨节点重放同一 requestId），保留期清理/不在本机历史窗口同样导致回查不到；
/// ③ **命令行的 actor 类别与 `origin.kind` 不符**：同一 requestId 上先由本机 CLI 发起、后被设备
///    重发（`owned_command` 的唯一键是 `(actor_kind, actor_id, request_id)`，跨 actor 复用同一
///    requestId 在 DDL 上合法），或 1.7 之前 `event_origin` 按 actor 瞬时状态分类留下的错配行。
#[tokio::test]
async fn sync_replay_fails_closed_when_a_history_row_cannot_be_attributed() {
    /// 一条 device 事件 + 可选的命令行（`None` = 这次提交不带 `owned_command` 行）。
    async fn seed_with(
        store: &SqliteStore,
        session: &SessionId,
        causation: Option<RequestId>,
        command: Option<Actor>,
    ) {
        store
            .commit(OwnedCommit {
                session: Some(session.clone()),
                at: at(1),
                expected_version: None,
                state: None,
                turns: Vec::new(),
                events: vec![event("turn.completed", EventOrigin::Device, causation)],
                interactions: Vec::new(),
                compacted: Vec::new(),
                idempotency: command.map(|actor| IdempotencyRecord {
                    actor,
                    request: request(),
                    command: "session.prompt".to_owned(),
                    kind: CommandKind::Mutation,
                    session: Some(session.clone()),
                    expected_version: None,
                    request_fingerprint: Digest::new(&digest_text("sync-replay-fail"))
                        .expect("digest"),
                    accepted_at: at(1),
                }),
                command_terminal: None,
                origin_epoch: Some(epoch()),
            })
            .await
            .expect("一条 device 事件");
    }

    // ① `causation` 为空。
    let dir = temp_dir("sync-replay-no-causation");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    seed_with(&store, &session, None, None).await;
    let error = store
        .read_view()
        .await
        .expect("read view")
        .sync_replay(None, ReplayLimit::new(10))
        .await
        .expect_err("causation 为空的 device 事件必须失败关闭");
    assert!(
        matches!(&error, PortError::Corrupt(_)),
        "causation 为空：得到 {error:?}"
    );

    // ② 命令行不在本机：`causation` 指向的 requestId 没有对应命令行。
    let dir = temp_dir("sync-replay-no-command");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    seed_with(&store, &session, Some(request()), None).await;
    let error = store
        .read_view()
        .await
        .expect("read view")
        .sync_replay(None, ReplayLimit::new(10))
        .await
        .expect_err("回查不到命令行的 device 事件必须失败关闭");
    assert!(
        matches!(&error, PortError::Corrupt(_)),
        "命令行不在本机：得到 {error:?}"
    );
    // 诊断文本点名是哪一类无法归因（投递层据此记 `sync.event_delivery_unavailable` 的同族诊断）。
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("device event cannot be attributed"),
        "诊断应说明无法归因：{rendered}"
    );

    // ③ 命令行的 actor 类别不符：本机 CLI 命令诱发了这条 device 事件。
    let dir = temp_dir("sync-replay-wrong-actor");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    seed_with(&store, &session, Some(request()), Some(Actor::LocalCli)).await;
    let error = store
        .read_view()
        .await
        .expect("read view")
        .sync_replay(None, ReplayLimit::new(10))
        .await
        .expect_err("actor 类别不符的 device 事件必须失败关闭");
    assert!(
        matches!(&error, PortError::Corrupt(_)),
        "actor 类别不符：得到 {error:?}"
    );

    // node 类同口径：命令行不在本机 / actor 类别不符都失败关闭（配对规则要求它带非空 nodeId）。
    for (label, command) in [
        ("命令不在本机", None),
        ("actor 类别不符", Some(Actor::LocalCli)),
    ] {
        let dir = temp_dir(&format!("sync-replay-node-{}", label.len()));
        let store = open(&dir).await;
        let session = create_session(&store).await;
        store
            .commit(OwnedCommit {
                session: Some(session.clone()),
                at: at(1),
                expected_version: None,
                state: None,
                turns: Vec::new(),
                events: vec![event(
                    "session.created",
                    EventOrigin::Node,
                    Some(node_request()),
                )],
                interactions: Vec::new(),
                compacted: Vec::new(),
                idempotency: command.map(|actor| IdempotencyRecord {
                    actor,
                    request: node_request(),
                    command: "session.create".to_owned(),
                    kind: CommandKind::Mutation,
                    session: Some(session.clone()),
                    expected_version: None,
                    request_fingerprint: Digest::new(&digest_text("sync-replay-node"))
                        .expect("digest"),
                    accepted_at: at(1),
                }),
                command_terminal: None,
                origin_epoch: Some(epoch()),
            })
            .await
            .expect("一条 node 事件");
        let error = store
            .read_view()
            .await
            .expect("read view")
            .sync_replay(None, ReplayLimit::new(10))
            .await
            .expect_err("无法归因的 node 事件必须失败关闭");
        assert!(
            matches!(&error, PortError::Corrupt(_)),
            "node 类（{label}）：得到 {error:?}"
        );
    }
}

/// 词表外的 `origin_kind` 是**列值损坏**：读面失败关闭（`Corrupt`），不得静默降级成别的来源。
///
/// §7.3 的 CHECK 把该列钉在词表里，正常写入路径写不出这个词表外取值；用例显式关掉该约束的检查来模拟
/// 「库里有绕过适配器的行」，验证读取侧的纵深防御。
#[tokio::test]
async fn sync_replay_fails_closed_on_unknown_origin_kind() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let dir = temp_dir("sync-replay-corrupt-origin");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    seed(&store, &session).await;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(dir.join(DATABASE_FILE)))
        .await
        .expect("connect");
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&pool)
        .await
        .expect("忽略 CHECK 以写入词表外取值");
    sqlx::query(
        "UPDATE owned_event SET origin_kind = 'robot' WHERE event_type = 'agent.message.delta'",
    )
    .execute(&pool)
    .await
    .expect("update");
    pool.close().await;

    let error = store
        .read_view()
        .await
        .expect("read view")
        .sync_replay(None, ReplayLimit::new(10))
        .await
        .expect_err("词表外的 origin_kind 必须失败关闭");
    assert!(
        matches!(&error, PortError::Corrupt(_)),
        "得到 {error:?}（不得降级成某个已知来源）"
    );
}

/// `owned_command.actor_id` 不是 canonical UUID（设备行）时同样是列值损坏（`Corrupt`），与
/// `find_request`/`command_record_from_row` 对同一列的既有口径一致——不是「没有设备 id」。
#[tokio::test]
async fn sync_replay_fails_closed_on_non_device_actor_id() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let dir = temp_dir("sync-replay-corrupt-actor");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    seed(&store, &session).await;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(dir.join(DATABASE_FILE)))
        .await
        .expect("connect");
    sqlx::query("UPDATE owned_command SET actor_id = 'not-a-uuid' WHERE actor_kind = 'device'")
        .execute(&pool)
        .await
        .expect("update");
    pool.close().await;

    let error = store
        .read_view()
        .await
        .expect("read view")
        .sync_replay(None, ReplayLimit::new(10))
        .await
        .expect_err("actor_id 不是设备 id 时必须失败关闭");
    assert!(
        matches!(&error, PortError::Corrupt(_)),
        "得到 {error:?}（不得降级成「没有设备 id」）"
    );
}

/// 节点行的 `actor_id` 必须是 `"{ownerNodeId}/{accessNodeId}"` 复合键：拆不开、或任一半不是
/// canonical UUID 都是列值损坏（同设备行的口径）。
#[tokio::test]
async fn sync_replay_fails_closed_on_non_node_actor_id() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let dir = temp_dir("sync-replay-corrupt-node-actor");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    seed(&store, &session).await;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(dir.join(DATABASE_FILE)))
        .await
        .expect("connect");
    // 拆不开的复合键（没有 `/`）。
    sqlx::query("UPDATE owned_command SET actor_id = 'not-a-pair' WHERE actor_kind = 'node'")
        .execute(&pool)
        .await
        .expect("update");
    pool.close().await;

    let error = store
        .read_view()
        .await
        .expect("read view")
        .sync_replay(None, ReplayLimit::new(10))
        .await
        .expect_err("actor_id 不是 node/access-node 复合键时必须失败关闭");
    assert!(
        matches!(&error, PortError::Corrupt(_)),
        "得到 {error:?}（不得降级成「没有 nodeId」）"
    );
}
