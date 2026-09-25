//! `AuditStore`（`append`/`query` over `owned_audit`）的行为用例。
//!
//! 逐条对应 `docs/CORE_PORTS_AND_STORAGE.md` §5.3 的端口形状、§7.3 的 `owned_audit` 列与 §11.6 第 6 条：
//!
//! | 合同 | 用例 |
//! |---|---|
//! | `append` 复用写集的列与取值编码（列清单不动） | `round_trip_preserves_every_column` |
//! | `(target_kind, target_id)` 的编解码 | `every_target_kind_round_trips` |
//! | `(actor_kind, actor_id)` 的编解码（审计列没有设备 scopes） | `device_actor_keeps_its_id_but_not_scopes` |
//! | `since`/`until` 含端点、时间区间过滤 | `time_window_includes_both_endpoints` |
//! | 无匹配条件（含 `since > until`）不是错误 | `unmatched_filters_return_empty_results`、`reversed_time_window_returns_no_rows` |
//! | `actions` 多值过滤（空 = 不过滤） | `actions_filter_accepts_multiple_values` |
//! | `actor`/`target` 过滤按存储列等值匹配 | `actor_and_target_filters_match_the_stored_columns` |
//! | `limit`；`None` = 不限 | `limit_caps_rows_and_none_means_unbounded` |
//! | 按 `at` 升序返回（与插入顺序无关） | `rows_come_back_in_ascending_time_order` |
//! | `AuditQuery::default()` 返回全部 | `default_query_returns_every_row` |
//! | 容量：新增行超限时拒绝写入（§7.5 ⑥） | `append_refuses_a_new_row_when_over_capacity` |
//! | 保留：追加的行受 §7.5 ⑤ 的审计 TTL 约束 | `appended_rows_are_swept_by_audit_retention` |
//!
//! 断言只针对可观察结果：端口返回值（[`AuditRecord`] 的逐字段访问器）与真实 SQLite 文件上的行为。

mod support;

use std::path::PathBuf;

use acp_core::model::{
    Actor, AuditAction, AuditOutcome, AuditRecord, DeviceId, Digest, EntityRef, ExportId, ImportId,
    InteractionId, NodeId, PairingId, PortError, RequestId, ScopeSet, SessionId, Timestamp, TurnId,
    UnavailableKind,
};
use acp_core::ports::{AuditQuery, AuditStore, RetentionPolicy, SessionStore};
use storage_sqlite::migrate::{DATABASE_FILE, StorageConfig};
use storage_sqlite::session_store::SqliteStore;
use support::{digest_text, measured_storage_bytes, raw_pool, temp_dir};

// ---------------------------------------------------------------------------------------------
// 固定输入
// ---------------------------------------------------------------------------------------------

const T0: &str = "2026-09-18T00:00:00.000Z";
const T1: &str = "2026-09-18T00:01:00.000Z";
const T2: &str = "2026-09-18T00:02:00.000Z";
const T3: &str = "2026-09-18T00:03:00.000Z";
/// 早于 `T0` 一年以上：用于让审计 TTL 窗口把它扫掉。
const OLD: &str = "2024-01-01T00:00:00.000Z";

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const REQUEST: &str = "22222222-2222-4222-8222-222222222222";
const TURN: &str = "33333333-3333-4333-8333-333333333333";
const INTERACTION: &str = "44444444-4444-4444-8444-444444444444";
const PAIRING: &str = "55555555-5555-4555-8555-555555555555";
const DEVICE: &str = "66666666-6666-4666-8666-666666666666";
const NODE: &str = "77777777-7777-4777-8777-777777777777";
const ACCESS_NODE: &str = "88888888-8888-4888-8888-888888888888";
const VIA_NODE: &str = "99999999-9999-4999-8999-999999999999";

const EXPORT: &str = "export-one";
const IMPORT: &str = "import-one";

// ---------------------------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------------------------

fn timestamp(text: &str) -> Timestamp {
    Timestamp::new(text).expect("timestamp")
}

fn at(text: &str) -> Timestamp {
    timestamp(text)
}

fn node_id() -> NodeId {
    NodeId::new(NODE).expect("node id")
}

fn access_node_id() -> NodeId {
    NodeId::new(ACCESS_NODE).expect("access node id")
}

fn session_id() -> SessionId {
    SessionId::new(SESSION).expect("session id")
}

fn request_id() -> RequestId {
    RequestId::new(REQUEST).expect("request id")
}

fn digest(seed: &str) -> Digest {
    Digest::new(&digest_text(seed)).expect("digest")
}

fn record(at_text: &str, action: AuditAction, actor: Actor, target: EntityRef) -> AuditRecord {
    AuditRecord::try_new(
        timestamp(at_text),
        action,
        actor,
        None,
        None,
        target,
        AuditOutcome::Success,
        None,
    )
    .expect("audit record")
}

fn cli_record(at_text: &str, action: AuditAction, target: EntityRef) -> AuditRecord {
    record(at_text, action, Actor::LocalCli, target)
}

/// 每个用例一个独立临时数据目录（`support::temp_dir` 已按 §7.1 建出 `0700` 目录）。
async fn open(name: &str) -> (SqliteStore, PathBuf) {
    let dir = temp_dir(name);
    let store = SqliteStore::open(StorageConfig::new(&dir), &at(T0))
        .await
        .expect("open store");
    (store, dir)
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

/// 读回全部行（`AuditQuery::default()` 的语义：不设任何过滤、不限行数）。
async fn all(store: &SqliteStore) -> Vec<AuditRecord> {
    store
        .query(AuditQuery::default())
        .await
        .expect("query all audit rows")
}

// ---------------------------------------------------------------------------------------------
// 往返
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn round_trip_preserves_every_column() {
    let (store, _dir) = open("audit-round-trip").await;
    let written = AuditRecord::try_new(
        timestamp(T2),
        AuditAction::NodeIdentityChanged,
        Actor::Node {
            node: node_id(),
            access_node: access_node_id(),
        },
        Some(NodeId::new(VIA_NODE).expect("via node")),
        Some("local-principal-ref".to_owned()),
        EntityRef::Command {
            session: Some(session_id()),
            request: request_id(),
        },
        AuditOutcome::Denied,
        Some(digest("detail")),
    )
    .expect("audit record");

    store.append(written.clone()).await.expect("append audit");
    let records = all(&store).await;

    assert_eq!(records, vec![written.clone()], "整条记录逐字段相等");
    assert_eq!(records[0].at(), written.at());
    assert_eq!(records[0].action(), written.action());
    assert_eq!(records[0].actor(), written.actor());
    assert_eq!(records[0].via_node(), written.via_node());
    assert_eq!(
        records[0].local_principal_ref(),
        written.local_principal_ref()
    );
    assert_eq!(records[0].target(), written.target());
    assert_eq!(records[0].outcome(), written.outcome());
    assert_eq!(records[0].detail_digest(), written.detail_digest());
    store.close().await;
}

#[tokio::test]
async fn every_target_kind_round_trips() {
    let (store, _dir) = open("audit-target-kinds").await;
    let targets = vec![
        EntityRef::Session(session_id()),
        EntityRef::Turn(TurnId::new(TURN).expect("turn id")),
        EntityRef::Interaction(InteractionId::new(INTERACTION).expect("interaction id")),
        EntityRef::Command {
            session: Some(session_id()),
            request: request_id(),
        },
        // 分离式命令：`target_id` 只有 request，没有 `session/` 前缀（§3.1 的 `EntityRef::target_id`）。
        EntityRef::Command {
            session: None,
            request: request_id(),
        },
        EntityRef::Pairing(PairingId::new(PAIRING).expect("pairing id")),
        EntityRef::Device(DeviceId::new(DEVICE).expect("device id")),
        EntityRef::Node(node_id()),
        EntityRef::Export(ExportId::new(EXPORT).expect("export id")),
        EntityRef::Import(ImportId::new(IMPORT).expect("import id")),
        EntityRef::Provider("openai.primary".to_owned()),
    ];
    for target in &targets {
        store
            .append(cli_record(
                T0,
                AuditAction::AuthorizationDenied,
                target.clone(),
            ))
            .await
            .expect("append audit");
    }

    let records = all(&store).await;
    let read_back: Vec<EntityRef> = records.iter().map(|row| row.target().clone()).collect();
    assert_eq!(read_back, targets, "每种实体类别的 (kind, id) 都原样读回");
    store.close().await;
}

#[tokio::test]
async fn device_actor_keeps_its_id_but_not_scopes() {
    let (store, _dir) = open("audit-device-actor").await;
    let scopes = ScopeSet::try_from_iter(["session.list", "session.read"]).expect("scopes");
    let written = record(
        T0,
        AuditAction::DeviceAuthenticated,
        Actor::Device {
            device: DeviceId::new(DEVICE).expect("device id"),
            scopes: scopes.clone(),
        },
        EntityRef::Device(DeviceId::new(DEVICE).expect("device id")),
    );
    store.append(written).await.expect("append audit");

    // §7.3 的审计列只有 `(actor_kind, actor_id)`：设备 scopes 不在表里，读回是空集合。
    // 审计只记录归因，授权由 core 判定，因此这是投影而不是缺陷。
    let records = all(&store).await;
    assert_eq!(
        records[0].actor(),
        &Actor::Device {
            device: DeviceId::new(DEVICE).expect("device id"),
            scopes: ScopeSet::empty(),
        }
    );
    assert_eq!(
        records[0].actor().device_id().expect("device id").as_str(),
        DEVICE
    );
    assert!(
        !records[0]
            .actor()
            .scopes()
            .expect("device scopes")
            .contains("session.read")
    );
    assert!(scopes.contains("session.read"), "原记录不受影响");
    store.close().await;
}

// ---------------------------------------------------------------------------------------------
// 过滤
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn time_window_includes_both_endpoints() {
    let (store, _dir) = open("audit-time-window").await;
    for (text, action) in [
        (T1, AuditAction::PairingCreated),
        (T2, AuditAction::PairingClaimed),
        (T3, AuditAction::PairingApproved),
    ] {
        store
            .append(cli_record(
                text,
                action,
                EntityRef::Pairing(PairingId::new(PAIRING).expect("pairing id")),
            ))
            .await
            .expect("append audit");
    }

    let window = store
        .query(AuditQuery {
            since: Some(at(T2)),
            until: Some(at(T3)),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert_eq!(
        window.iter().map(AuditRecord::action).collect::<Vec<_>>(),
        vec![AuditAction::PairingClaimed, AuditAction::PairingApproved],
        "since/until 两端都包含"
    );

    let open_ended = store
        .query(AuditQuery {
            since: Some(at(T3)),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert_eq!(open_ended.len(), 1, "只给 since 时只按该端点过滤");

    let prefix = store
        .query(AuditQuery {
            until: Some(at(T1)),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert_eq!(prefix.len(), 1, "只给 until 时只按该端点过滤");
    store.close().await;
}

#[tokio::test]
async fn reversed_time_window_returns_no_rows() {
    let (store, _dir) = open("audit-reversed-window").await;
    store
        .append(cli_record(
            T2,
            AuditAction::PairingCreated,
            EntityRef::Pairing(PairingId::new(PAIRING).expect("pairing id")),
        ))
        .await
        .expect("append audit");

    // `since > until`：区间为空集，不是调用方的参数错误，返回空 `Vec`。
    let records = store
        .query(AuditQuery {
            since: Some(at(T3)),
            until: Some(at(T1)),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert!(records.is_empty());
    store.close().await;
}

#[tokio::test]
async fn unmatched_filters_return_empty_results() {
    let (store, _dir) = open("audit-unmatched").await;
    store
        .append(cli_record(
            T0,
            AuditAction::ProviderConfigured,
            EntityRef::Provider("openai.primary".to_owned()),
        ))
        .await
        .expect("append audit");

    let by_action = store
        .query(AuditQuery {
            actions: vec![AuditAction::DeviceRevoked],
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert!(by_action.is_empty(), "没有该动作的行 → 空结果");

    let by_target = store
        .query(AuditQuery {
            target: Some(EntityRef::Provider("other.provider".to_owned())),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert!(by_target.is_empty(), "没有该目标的行 → 空结果");

    let by_actor = store
        .query(AuditQuery {
            actor: Some(Actor::LocalCli),
            target: Some(EntityRef::Device(DeviceId::new(DEVICE).expect("device id"))),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert!(by_actor.is_empty(), "actor 与 target 是合取条件");
    store.close().await;
}

#[tokio::test]
async fn actions_filter_accepts_multiple_values() {
    let (store, _dir) = open("audit-actions").await;
    for action in [
        AuditAction::PairingCreated,
        AuditAction::DeviceRevoked,
        AuditAction::ExportRevoked,
        AuditAction::NodeTrustRevoked,
    ] {
        store
            .append(cli_record(T0, action, EntityRef::Provider("p".to_owned())))
            .await
            .expect("append audit");
    }

    let filtered = store
        .query(AuditQuery {
            actions: vec![AuditAction::DeviceRevoked, AuditAction::ExportRevoked],
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert_eq!(
        filtered.iter().map(AuditRecord::action).collect::<Vec<_>>(),
        vec![AuditAction::DeviceRevoked, AuditAction::ExportRevoked]
    );

    let unfiltered = store
        .query(AuditQuery {
            actions: Vec::new(),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert_eq!(unfiltered.len(), 4, "空 actions = 不按动作过滤");
    store.close().await;
}

#[tokio::test]
async fn actor_and_target_filters_match_the_stored_columns() {
    let (store, _dir) = open("audit-actor-target").await;
    let local_target = EntityRef::Session(session_id());
    let remote_target = EntityRef::Node(node_id());
    store
        .append(cli_record(
            T1,
            AuditAction::ImportAdded,
            local_target.clone(),
        ))
        .await
        .expect("append audit");
    store
        .append(record(
            T2,
            AuditAction::ImportAdded,
            Actor::Node {
                node: node_id(),
                access_node: access_node_id(),
            },
            remote_target.clone(),
        ))
        .await
        .expect("append audit");

    let by_target = store
        .query(AuditQuery {
            target: Some(remote_target),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert_eq!(by_target.len(), 1);
    assert_eq!(by_target[0].actor().kind().as_str(), "node");

    let by_actor = store
        .query(AuditQuery {
            actor: Some(Actor::LocalCli),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert_eq!(by_actor.len(), 1);
    assert_eq!(by_actor[0].target(), &local_target);
    assert_eq!(by_actor[0].actor().id_text(), "cli");

    // Node 的复合键是 `"{node}/{access_node}"`（§7.3）：access node 参与匹配。
    let wrong_access_node = store
        .query(AuditQuery {
            actor: Some(Actor::Node {
                node: node_id(),
                access_node: node_id(),
            }),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert!(wrong_access_node.is_empty());
    store.close().await;
}

// ---------------------------------------------------------------------------------------------
// 排序、limit 与默认查询
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn rows_come_back_in_ascending_time_order() {
    let (store, _dir) = open("audit-order").await;
    // 故意按时间倒序插入：顺序只能来自 `ORDER BY at`，不能来自 `audit_id`（插入顺序）。
    for (text, action) in [
        (T3, AuditAction::PairingApproved),
        (T1, AuditAction::PairingCreated),
        (T2, AuditAction::PairingClaimed),
    ] {
        store
            .append(cli_record(
                text,
                action,
                EntityRef::Pairing(PairingId::new(PAIRING).expect("pairing id")),
            ))
            .await
            .expect("append audit");
    }

    let records = all(&store).await;
    assert_eq!(
        records.iter().map(AuditRecord::action).collect::<Vec<_>>(),
        vec![
            AuditAction::PairingCreated,
            AuditAction::PairingClaimed,
            AuditAction::PairingApproved
        ]
    );
    let times: Vec<&str> = records.iter().map(|row| row.at().as_str()).collect();
    let mut sorted = times.clone();
    sorted.sort_unstable();
    assert_eq!(times, sorted, "按 at 升序");
    store.close().await;
}

#[tokio::test]
async fn limit_caps_rows_and_none_means_unbounded() {
    let (store, _dir) = open("audit-limit").await;
    for (text, action) in [
        (T1, AuditAction::PairingCreated),
        (T2, AuditAction::PairingClaimed),
        (T3, AuditAction::PairingApproved),
    ] {
        store
            .append(cli_record(
                text,
                action,
                EntityRef::Pairing(PairingId::new(PAIRING).expect("pairing id")),
            ))
            .await
            .expect("append audit");
    }

    let limited = store
        .query(AuditQuery {
            limit: Some(2),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert_eq!(
        limited.iter().map(AuditRecord::action).collect::<Vec<_>>(),
        vec![AuditAction::PairingCreated, AuditAction::PairingClaimed],
        "limit 取升序的前 n 行"
    );

    let zero = store
        .query(AuditQuery {
            limit: Some(0),
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert!(zero.is_empty(), "limit = 0 是上限而不是「不限」");

    let unbounded = store
        .query(AuditQuery {
            limit: None,
            ..AuditQuery::default()
        })
        .await
        .expect("query");
    assert_eq!(unbounded.len(), 3, "limit = None 不限行数（合同 §5.3）");
    store.close().await;
}

#[tokio::test]
async fn default_query_returns_every_row() {
    let (store, _dir) = open("audit-default-query").await;
    // 跨动作、跨 actor、跨时间：默认查询不施加任何过滤（`actions` 为空 = 不按动作过滤）。
    store
        .append(cli_record(
            T1,
            AuditAction::PairingCreated,
            EntityRef::Pairing(PairingId::new(PAIRING).expect("pairing id")),
        ))
        .await
        .expect("append audit");
    store
        .append(record(
            T2,
            AuditAction::AuthorizationDenied,
            Actor::Node {
                node: node_id(),
                access_node: access_node_id(),
            },
            EntityRef::Command {
                session: None,
                request: request_id(),
            },
        ))
        .await
        .expect("append audit");

    let records = store.query(AuditQuery::default()).await.expect("query");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].at(), &at(T1));
    assert_eq!(records[1].at(), &at(T2));
    store.close().await;
}

// ---------------------------------------------------------------------------------------------
// 容量与保留
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn append_refuses_a_new_row_when_over_capacity() {
    let (store, dir) = open("audit-capacity").await;
    store
        .append(cli_record(
            T0,
            AuditAction::PairingCreated,
            EntityRef::Pairing(PairingId::new(PAIRING).expect("pairing id")),
        ))
        .await
        .expect("append audit");
    let path = dir.join(DATABASE_FILE);
    store.close().await;

    let pool = raw_pool(&path).await;
    let measured = measured_storage_bytes(&pool).await;
    pool.close().await;

    // 上限恰好等于现值：再写一行就超限，容量门按 §7.5 ⑥ 拒绝新写入而不是静默丢弃审计。
    let mut config = StorageConfig::new(&dir);
    config.max_total_size_bytes = measured;
    let store = SqliteStore::open(config, &at(T1)).await.expect("reopen");
    match store
        .append(cli_record(
            T1,
            AuditAction::PairingClaimed,
            EntityRef::Pairing(PairingId::new(PAIRING).expect("pairing id")),
        ))
        .await
    {
        Err(PortError::Unavailable(kind)) => assert_eq!(kind, UnavailableKind::StorageFull),
        other => panic!("expected Unavailable(StorageFull), got {other:?}"),
    }
    assert_eq!(all(&store).await.len(), 1, "被拒绝的写不留下半行");
    store.close().await;
}

#[tokio::test]
async fn appended_rows_are_swept_by_audit_retention() {
    let (store, _dir) = open("audit-retention").await;
    store
        .append(cli_record(
            OLD,
            AuditAction::PairingExpired,
            EntityRef::Pairing(PairingId::new(PAIRING).expect("pairing id")),
        ))
        .await
        .expect("append audit");
    store
        .append(cli_record(
            T1,
            AuditAction::PairingCreated,
            EntityRef::Pairing(PairingId::new(PAIRING).expect("pairing id")),
        ))
        .await
        .expect("append audit");
    assert_eq!(all(&store).await.len(), 2);

    // §7.5 ⑤：审计按 `storage.audit_retention_days`（365 天）过期；追加的行与写集写的行同表同规则。
    let report = store.prune(policy(), timestamp(T0)).await.expect("prune");
    assert_eq!(report.removed_audit, 1);

    let remaining = all(&store).await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].action(), AuditAction::PairingCreated);
    store.close().await;
}
