//! §9.5、§9.6、§9.14：imported 家族的去重/失效/无正文（黄金列清单）与审计保留。
//!
//! 另含 admin-state-persistence「撤销与删除后的写入不得复活资源」的回归：
//! `late_callbacks_after_a_full_removal_cannot_rebuild_the_index`（§11.2 第 5 条、§5.2 约束）。

mod support;

use acp_core::model::{
    Digest, EntityRef, EventId, EventType, ExportId, GrantSet, ImportId, ImportRecord, LocalCursor,
    NodeId, OriginCursor, OriginEpoch, PortError, Sequence, SessionId, Timestamp,
};
use acp_core::ports::{
    DeliveryReceipt, ExportStore, ImportRemoval, ImportWrite, ImportedSessionQuery,
    RemoteDeliveryStore, ReplayLimit, RetentionPolicy, StateChange, WriteContext,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

fn at(hour: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{hour:02}:00:00.000Z")).expect("timestamp")
}

fn node() -> NodeId {
    NodeId::new("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").expect("node")
}

fn export() -> ExportId {
    ExportId::new("export-one").expect("export")
}

fn session() -> SessionId {
    SessionId::new("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").expect("session")
}

fn remote() -> acp_core::model::RemoteSessionRef {
    acp_core::model::RemoteSessionRef::new(node(), export(), session())
}

fn origin(event: &str) -> acp_core::model::OriginEventRef {
    acp_core::model::OriginEventRef::new(
        node(),
        OriginEpoch::new("cccccccc-cccc-4ccc-8ccc-cccccccccccc").expect("epoch"),
        EventId::new(event).expect("event id"),
    )
}

fn digest(seed: &str) -> Digest {
    Digest::new(&support::digest_text(seed)).expect("digest")
}

fn receipt(event: &str, sequence: u64, seed: &str, hour: u32) -> DeliveryReceipt {
    DeliveryReceipt {
        session: remote(),
        origin: origin(event),
        origin_sequence: Sequence::new(sequence).expect("sequence"),
        event_type: EventType::new("resource.event").expect("event type"),
        payload_digest: digest(seed),
        at: at(hour),
    }
}

async fn store(dir: &std::path::Path) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at(0))
        .await
        .expect("open store")
}

/// 本文件的 Import 标识（`seed_import`、`drop_import`、`remove_import` 共用）。
fn import() -> ImportId {
    ImportId::new("import-one").expect("import id")
}

/// 先登记 Import 与其 Export 关联行：§11.2 第 5 条/§7.4 起，imported 写路径必须先在
/// `imported_import_export` 里有归属，否则被拒——因此夹具必须先建档，这也让每条用例的起点与生产
/// 路径一致（先 `import.add`，后有会话同步）。
async fn seed_import(store: &SqliteStore) {
    store
        .add_import(ImportWrite {
            record: ImportRecord::try_new(
                import(),
                "wss://owner.example/acpr",
                node(),
                vec![export()],
                GrantSet::try_from_iter(["grant.remote-work"]).expect("grants"),
            )
            .expect("import record"),
            exports: vec![export()],
            context: WriteContext {
                at: at(0),
                audit: Vec::new(),
            },
        })
        .await
        .expect("add import");
}

/// 建一次 imported 会话行（归属行由 `seed_import` 先落库）。
async fn seed_session(store: &SqliteStore) {
    seed_import(store).await;
    store
        .upsert_session(
            acp_core::ports::ImportedSessionRecord {
                session: remote(),
                origin_epoch: Some(
                    OriginEpoch::new("cccccccc-cccc-4ccc-8ccc-cccccccccccc").expect("epoch"),
                ),
                title: Some("imported".to_owned()),
                agent: None,
                state: None,
                version: None,
                created_at: Some(at(0)),
                last_origin_sequence: None,
                acked: None,
                attachment: None,
                updated_at: at(0),
            },
            at(0),
        )
        .await
        .expect("upsert session");
}

/// §9.6 的黄金列清单：`imported_*` 的列集合必须与 §7.4 冻结的清单逐项相等。
#[tokio::test]
async fn imported_tables_match_the_frozen_column_lists() {
    let dir = temp_dir("imported-columns");
    let store = store(&dir).await;
    store.close().await;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let expected: [(&str, &[&str]); 6] = [
        (
            "imported_import",
            &[
                "import_id",
                "owner_node_id",
                "display_name",
                "endpoint_ref",
                "cache_policy",
                "owner_server_epoch",
                "created_at",
                "removed_at",
                "grants_json",
            ],
        ),
        (
            "imported_import_export",
            &["import_id", "owner_node_id", "export_id", "added_at"],
        ),
        (
            "imported_session",
            &[
                "owner_node_id",
                "export_id",
                "session_id",
                "title",
                "agent_id",
                "agent_name",
                "state",
                "version",
                "created_at",
                "last_origin_epoch",
                "last_origin_sequence",
                "acked_origin_epoch",
                "acked_origin_sequence",
                "next_local_sequence",
                "attachment_id",
                "attachment_generation",
                "updated_at",
            ],
        ),
        (
            "imported_delivery_index",
            &[
                "owner_node_id",
                "export_id",
                "session_id",
                "origin_event_id",
                "origin_epoch",
                "origin_sequence",
                "local_sequence",
                "event_type",
                "payload_digest",
                "received_at",
            ],
        ),
        (
            "imported_command_ref",
            &[
                "owner_node_id",
                "export_id",
                "session_id",
                "request_id",
                "command",
                "status",
                "accepted_at",
                "terminal_at",
                "terminal_event_id",
                "error_code",
                "retryable",
            ],
        ),
        (
            "imported_audit",
            &[
                "audit_id",
                "at",
                "action",
                "actor_kind",
                "actor_id",
                "owner_node_id",
                "export_id",
                "session_id",
                "request_id",
                "local_principal_ref",
                "target_kind",
                "target_id",
                "outcome",
                "detail_digest",
            ],
        ),
    ];
    for (table, columns) in expected {
        assert_eq!(
            column_names(&pool, table).await,
            columns,
            "{table} must match §7.4 exactly (no extra content column may be added)"
        );
    }
    pool.close().await;
}

/// §9.5：同一 `origin_event_id` 重发只产生一行，且不新增 `local_sequence`。
#[tokio::test]
async fn duplicate_receipts_do_not_advance_the_local_sequence() {
    let dir = temp_dir("imported-dedup");
    let store = store(&dir).await;
    seed_session(&store).await;

    let first = store
        .commit_receipt(receipt(
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            1,
            "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE",
            1,
        ))
        .await
        .expect("first receipt");
    assert!(!first.duplicate);
    assert_eq!(first.local_sequence, LocalCursor::new(1));

    let again = store
        .commit_receipt(receipt(
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            1,
            "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE",
            2,
        ))
        .await
        .expect("duplicate receipt");
    assert!(again.duplicate);
    assert_eq!(again.local_sequence, first.local_sequence);

    let second = store
        .commit_receipt(receipt(
            "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
            2,
            "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
            2,
        ))
        .await
        .expect("second event");
    assert!(!second.duplicate);
    assert_eq!(second.local_sequence, LocalCursor::new(2));

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM imported_delivery_index").await,
        2
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(DISTINCT local_sequence) FROM imported_delivery_index"
        )
        .await,
        2
    );
    pool.close().await;

    // `local_replay` 只返回无正文索引条目。
    let entries = store
        .local_replay(None, ReplayLimit::default())
        .await
        .expect("local replay");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].local_sequence, LocalCursor::new(1));
    assert_eq!(
        entries[0].origin.origin_event_id.as_str(),
        "dddddddd-dddd-4ddd-8ddd-dddddddddddd"
    );
    assert_eq!(entries[0].session, remote());
    let after = store
        .local_replay(Some(LocalCursor::new(1)), ReplayLimit::default())
        .await
        .expect("after cursor");
    assert_eq!(after.len(), 1);
    store.close().await;
}

/// 会话行不存在时 `commit_receipt` 返回 `NotFound(Session)`，且不推进 `next_local_sequence`。
/// 夹具必须先 `seed_import`：归属行在、会话行不在（否则会先被归属前置拦下，就测不到这个分支）。
#[tokio::test]
async fn receipt_without_imported_session_is_rejected() {
    let dir = temp_dir("imported-orphan");
    let store = store(&dir).await;
    seed_import(&store).await;
    let error = store
        .commit_receipt(receipt(
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            1,
            "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE",
            1,
        ))
        .await
        .expect_err("orphan receipt");
    assert!(
        matches!(&error, PortError::NotFound(EntityRef::Session(id)) if id == &session()),
        "缺会话行必须报 NotFound(Session)，实际：{error:?}"
    );

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM imported_delivery_index").await,
        0
    );
    pool.close().await;
    store.close().await;
}

/// §9.5：`drop_import` 之后交付索引与命令引用为空，而审计行仍在。
#[tokio::test]
async fn drop_import_keeps_audit_rows() {
    let dir = temp_dir("imported-drop");
    let store = store(&dir).await;
    seed_session(&store).await;
    store
        .commit_receipt(receipt(
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            1,
            "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE",
            1,
        ))
        .await
        .expect("receipt");

    // 命令引用与审计行（审计由 `AuditStore` 写入，本切片直接落行以验证保留义务；Import 管理行与
    // Export 关联行已由 `seed_session` 经 `add_import` 落库）。
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    sqlx::query(
        "INSERT INTO imported_command_ref (owner_node_id, export_id, session_id, request_id, \
         command, status, accepted_at) VALUES (?1, ?2, ?3, ?4, 'session.prompt', 'accepted', ?5)",
    )
    .bind(node().as_str())
    .bind(export().as_str())
    .bind(session().as_str())
    .bind("11111111-1111-4111-8111-111111111111")
    .bind(at(1).as_str())
    .execute(&pool)
    .await
    .expect("command ref");
    sqlx::query(
        "INSERT INTO imported_audit (at, action, actor_kind, actor_id, owner_node_id, export_id, \
         session_id, target_kind, target_id, outcome) \
         VALUES (?1, 'node.trust_revoked', 'node', ?2, ?2, ?3, ?4, 'node', ?2, 'success')",
    )
    .bind(at(1).as_str())
    .bind(node().as_str())
    .bind(export().as_str())
    .bind(session().as_str())
    .execute(&pool)
    .await
    .expect("audit row");
    pool.close().await;

    let report = store.drop_import(&import()).await.expect("drop import");
    assert_eq!(report.delivery_index_removed, 1);
    assert_eq!(report.command_refs_removed, 1);

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM imported_delivery_index").await,
        0
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM imported_command_ref").await,
        0
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM imported_audit").await,
        1,
        "audit rows must survive drop_import"
    );
    pool.close().await;

    // 未知 import → NotFound。
    let missing = store
        .drop_import(&ImportId::new("nope").expect("import id"))
        .await
        .expect_err("unknown import");
    assert!(matches!(missing, acp_core::model::PortError::NotFound(_)));
    store.close().await;
}

/// 规格：完整移除后的迟到回调不能重建索引（admin-state-persistence）。
///
/// `remove_import` 在同一事务删掉管理行、关联行与 `imported_session`（级联交付索引/命令引用）；之后
/// 到达的 `upsert_session` 与 `commit_receipt` 都必须在写任何行之前失败关闭——否则会话行会被重建，
/// 收据的复合外键随之重新成立，已删除的交付索引就被“部分复活”了。错误定位取 `Export`：关联行
/// 已删，调用方回推不出 `importId`。
#[tokio::test]
async fn late_callbacks_after_a_full_removal_cannot_rebuild_the_index() {
    let dir = temp_dir("imported-removed-callback");
    let store = store(&dir).await;
    seed_session(&store).await;
    store
        .commit_receipt(receipt(
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            1,
            "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE",
            1,
        ))
        .await
        .expect("receipt");

    store
        .remove_import(ImportRemoval {
            import: import(),
            context: WriteContext {
                at: at(2),
                audit: Vec::new(),
            },
        })
        .await
        .expect("remove import");

    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_pool(&path).await;
    let mut before = Vec::new();
    for table in [
        "imported_import_export",
        "imported_session",
        "imported_delivery_index",
    ] {
        before.push(scalar_i64(&pool, &format!("SELECT COUNT(*) FROM {table}")).await);
    }
    assert_eq!(
        before,
        vec![0, 0, 0],
        "完整移除后关联行、会话行与交付索引都为空"
    );

    // 迟到的会话同步被拒：不得重建会话行。
    let late = store
        .upsert_session(
            acp_core::ports::ImportedSessionRecord {
                session: remote(),
                origin_epoch: Some(
                    OriginEpoch::new("cccccccc-cccc-4ccc-8ccc-cccccccccccc").expect("epoch"),
                ),
                title: Some("late".to_owned()),
                agent: None,
                state: None,
                version: None,
                created_at: Some(at(0)),
                last_origin_sequence: None,
                acked: None,
                attachment: None,
                updated_at: at(3),
            },
            at(3),
        )
        .await
        .expect_err("late upsert must be rejected");
    assert!(
        matches!(&late, PortError::NotFound(EntityRef::Export(id)) if id == &export()),
        "迟到的会话同步必须报 NotFound(Export)，实际：{late:?}"
    );

    // 迟到的交付收据被拒：同一不变量的第二个入口单独断言。
    let late_receipt = store
        .commit_receipt(receipt(
            "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
            2,
            "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
            3,
        ))
        .await
        .expect_err("late receipt must be rejected");
    assert!(
        matches!(&late_receipt, PortError::NotFound(EntityRef::Export(id)) if id == &export()),
        "迟到的交付收据必须报 NotFound(Export)，实际：{late_receipt:?}"
    );

    for table in [
        "imported_import",
        "imported_import_export",
        "imported_session",
        "imported_delivery_index",
        "imported_command_ref",
    ] {
        assert_eq!(
            scalar_i64(&pool, &format!("SELECT COUNT(*) FROM {table}")).await,
            0,
            "{table} 必须在迟到回调被拒后仍为空"
        );
    }
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM imported_audit").await,
        0,
        "迟到回调不得写入任何行（含审计）"
    );
    pool.close().await;
    store.close().await;
}

/// §9.5：owner 的 origin epoch 变化后该会话的交付索引被清空，ACK 一并失效。
#[tokio::test]
async fn epoch_change_invalidates_the_delivery_index() {
    let dir = temp_dir("imported-epoch");
    let store = store(&dir).await;
    seed_session(&store).await;
    store
        .commit_receipt(receipt(
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            1,
            "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE",
            1,
        ))
        .await
        .expect("receipt");
    let acked = store
        .ack(
            &remote(),
            OriginCursor::new(
                OriginEpoch::new("cccccccc-cccc-4ccc-8ccc-cccccccccccc").expect("epoch"),
                Sequence::new(1).expect("sequence"),
            ),
            at(2),
        )
        .await
        .expect("ack");
    assert!(acked.applied);
    assert!(store.load_ack(&remote()).await.expect("load ack").is_some());

    // 同 epoch 的回退请求被忽略。
    let back = store
        .ack(
            &remote(),
            OriginCursor::new(
                OriginEpoch::new("cccccccc-cccc-4ccc-8ccc-cccccccccccc").expect("epoch"),
                Sequence::new(1).expect("sequence"),
            ),
            at(3),
        )
        .await
        .expect("ack");
    assert!(!back.applied, "a non-advancing ack must be ignored");

    // Owner 重建了事件库 → 新 epoch。
    store
        .upsert_session(
            acp_core::ports::ImportedSessionRecord {
                session: remote(),
                origin_epoch: Some(
                    OriginEpoch::new("99999999-9999-4999-8999-999999999999").expect("epoch"),
                ),
                title: None,
                agent: None,
                state: None,
                version: None,
                created_at: None,
                last_origin_sequence: None,
                acked: None,
                attachment: None,
                updated_at: at(4),
            },
            at(4),
        )
        .await
        .expect("epoch change");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM imported_delivery_index").await,
        0,
        "a new origin epoch invalidates every cursor of that session"
    );
    pool.close().await;
    assert!(store.load_ack(&remote()).await.expect("load ack").is_none());
    store.close().await;
}

/// §9.6 的行为断言：owned 正文里的标记串绝不出现在任何 `imported_*` 表里。
#[tokio::test]
async fn imported_tables_never_hold_owned_content() {
    use acp_core::model::{AgentId, AgentRef, EventKind, StoredPolicy};
    use acp_core::ports::{OwnedCommit, SessionStore as _};

    const MARKER: &str = "secret-prompt-marker-4f2a";
    let dir = temp_dir("imported-nocontent");
    let store = store(&dir).await;
    seed_session(&store).await;

    let created = store
        .commit(OwnedCommit {
            session: None,
            at: at(0),
            expected_version: None,
            state: Some(StateChange::Create(acp_core::ports::NewSession {
                title: Some("owned".to_owned()),
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
            origin_epoch: Some(
                OriginEpoch::new("6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b").expect("epoch"),
            ),
        })
        .await
        .expect("create owned session");
    let owned = created.session_id.expect("session id");
    store
        .commit(OwnedCommit {
            session: Some(owned),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![acp_core::model::PendingEvent::new(
                EventKind::FinalMessage,
                EventType::new("agent.message").expect("event type"),
                StoredPolicy::Durable,
                acp_core::model::EventPayload::new(
                    acp_core::model::ViewJson::new(&format!(r#"{{"text":"{MARKER}"}}"#))
                        .expect("view"),
                    None,
                ),
                acp_core::model::EventOrigin::Agent,
                None,
                None,
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(
                OriginEpoch::new("6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b").expect("epoch"),
            ),
        })
        .await
        .expect("owned event");
    store
        .commit_receipt(receipt(
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            1,
            "EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE",
            2,
        ))
        .await
        .expect("receipt");
    store.close().await;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    // 标记串确实在 owned 侧落库（否则本测试的断言是空的）。
    assert_eq!(
        scalar_i64(
            &pool,
            &format!("SELECT COUNT(*) FROM owned_event WHERE payload_json LIKE '%{MARKER}%'")
        )
        .await,
        1
    );
    for table in [
        "imported_import",
        "imported_import_export",
        "imported_session",
        "imported_delivery_index",
        "imported_command_ref",
        "imported_audit",
    ] {
        let columns = column_names(&pool, table).await;
        let haystacks: Vec<String> = columns
            .iter()
            .map(|column| format!("COALESCE(CAST({column} AS TEXT), '')"))
            .collect();
        let sql = format!(
            "SELECT COUNT(*) FROM {table} WHERE ({} || '') LIKE '%{MARKER}%'",
            haystacks.join(" || ")
        );
        assert_eq!(
            scalar_i64(&pool, &sql).await,
            0,
            "{table} must never contain owned content"
        );
    }
    pool.close().await;
}

/// 会话列表按 `imports`/`only`/`limit` 过滤。
#[tokio::test]
async fn imported_session_queries_filter_as_documented() {
    let dir = temp_dir("imported-queries");
    let store = store(&dir).await;
    seed_session(&store).await;

    let all = store
        .list_sessions(ImportedSessionQuery::default())
        .await
        .expect("list");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].session, remote());
    assert_eq!(all[0].updated_at, at(0));

    let by_reference = store
        .list_sessions(ImportedSessionQuery {
            imports: None,
            only: Some(vec![remote()]),
            limit: None,
        })
        .await
        .expect("by reference");
    assert_eq!(by_reference.len(), 1);

    let by_unknown_reference = store
        .list_sessions(ImportedSessionQuery {
            imports: None,
            only: Some(vec![acp_core::model::RemoteSessionRef::new(
                node(),
                export(),
                SessionId::new("99999999-9999-4999-8999-999999999999").expect("session"),
            )]),
            limit: None,
        })
        .await
        .expect("unknown reference");
    assert!(by_unknown_reference.is_empty());

    let empty_filter = store
        .list_sessions(ImportedSessionQuery {
            imports: Some(Vec::new()),
            only: None,
            limit: None,
        })
        .await
        .expect("empty filter");
    assert!(empty_filter.is_empty());

    let zero_limit = store
        .list_sessions(ImportedSessionQuery {
            imports: None,
            only: None,
            limit: Some(0),
        })
        .await
        .expect("zero limit");
    assert!(zero_limit.is_empty());
    store.close().await;
}

/// `find_request` 只返回无正文的终态引用（错误只有码与可重试标志）。
#[tokio::test]
async fn imported_command_refs_carry_no_message() {
    use acp_core::model::RequestId;

    let dir = temp_dir("imported-command-ref");
    let store = store(&dir).await;
    seed_session(&store).await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    sqlx::query(
        "INSERT INTO imported_command_ref (owner_node_id, export_id, session_id, request_id, \
         command, status, accepted_at, terminal_at, terminal_event_id, error_code, retryable) \
         VALUES (?1, ?2, ?3, ?4, 'session.prompt', 'failed', ?5, ?5, ?6, 'agent.refused', 0)",
    )
    .bind(node().as_str())
    .bind(export().as_str())
    .bind(session().as_str())
    .bind("11111111-1111-4111-8111-111111111111")
    .bind(at(1).as_str())
    .bind("dddddddd-dddd-4ddd-8ddd-dddddddddddd")
    .execute(&pool)
    .await
    .expect("command ref");
    pool.close().await;

    // `RemoteDeliveryStore::find_remote_request`（§5.2 v0.3 改名，不再与 `SessionStore::find_request` 同名）。
    let found = RemoteDeliveryStore::find_remote_request(
        &store,
        &remote(),
        &RequestId::new("11111111-1111-4111-8111-111111111111").expect("request"),
    )
    .await
    .expect("find")
    .expect("row");
    assert_eq!(found.command, "session.prompt");
    assert_eq!(found.status, acp_core::model::CommandStatus::Failed);
    let error = found.error.expect("error");
    assert_eq!(error.code(), "agent.refused");
    assert_eq!(error.message(), "", "imported rows carry no message");
    assert!(!error.retryable());
    store.close().await;
}

/// §7.5：Access-only 节点（没有 owned 会话）也必须能回收——度量含 `imported_*` 家族，`prune` 的循环才会
/// 真的删行；⑤ 的审计清理对 `imported_audit` 同样生效（只有 owned 家族进度量时这里会一行不删）。
#[tokio::test]
async fn prune_reclaims_the_imported_family_and_expired_audit() {
    let dir = temp_dir("imported-prune");
    let store = store(&dir).await;
    seed_session(&store).await;
    for index in 0..16_u64 {
        let event = format!("dddddddd-dddd-4ddd-8ddd-{index:012}");
        let seed = format!("seed-{index}");
        store
            .commit_receipt(receipt(&event, index + 1, &seed, 1))
            .await
            .expect("receipt");
    }

    // 审计行由原始 SQL 落库（`AuditStore` 不在本增量内）。一条早已过期、一条新鲜。
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    for at_text in ["2020-01-01T00:00:00.000Z", "2026-09-18T01:00:00.000Z"] {
        sqlx::query(
            "INSERT INTO imported_audit (at, action, actor_kind, actor_id, outcome, target_kind, target_id) \
             VALUES (?1, 'device.authenticated', 'device', 'device-one', 'success', 'device', 'device-one')",
        )
        .bind(at_text)
        .execute(&pool)
        .await
        .expect("audit row");
    }
    pool.close().await;

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM imported_delivery_index")
        .fetch_one(&raw_pool(&path).await)
        .await
        .expect("count");
    assert!(before > 0, "precondition: 交付索引必须非空");

    let policy = RetentionPolicy {
        transcript_retention_days: 90,
        sync_event_retention_days: 7,
        audit_retention_days: 365,
        max_total_size_bytes: 1,
        max_session_size_bytes: 1 << 20,
        persist_deltas: false,
    };
    let report = store.prune(policy, at(3)).await.expect("prune");

    assert!(
        report.removed_events > 0,
        "Access-only 节点必须真的回收 imported 行（旧口径下这里恒为 0）"
    );
    let index_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM imported_delivery_index")
        .fetch_one(&raw_pool(&path).await)
        .await
        .expect("count");
    assert_eq!(index_rows, 0, "容量压力下交付索引必须被清空");
    let expired_audit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM imported_audit WHERE at <= '2020-01-02T00:00:00.000Z'",
    )
    .fetch_one(&raw_pool(&path).await)
    .await
    .expect("count");
    assert_eq!(expired_audit, 0, "过期的 imported_audit 必须被 ⑤ 清掉");
    let fresh_audit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM imported_audit WHERE at > '2020-01-02T00:00:00.000Z'",
    )
    .fetch_one(&raw_pool(&path).await)
    .await
    .expect("count");
    assert_eq!(fresh_audit, 1, "未到期的审计行必须保留（审计保留义务）");
    store.close().await;
}
