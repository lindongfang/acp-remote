//! 会话历史读（本文件 §9 判据 33、§5.2 的对应 `[决定]`）：条目带持久化正文、按会话内序号分页。
//!
//! 覆盖：条目集合只取 `SYNC_PROTOCOL.md` §10.2.1 的 17 类（筛选与 `limit`/页标记在同一条语句里生效）、
//! 一次取行连正文（`view` 与库内 `payload_json` 逐字节相同）、不重不漏、同一游标同一结果、
//! 已压缩行不出现、`messages` 与 `history` 在同一请求里各自分页、原文不可用时带已登记原因、
//! 游标早于仍可读下界时显式报错。

mod support;

use acp_core::broker::HISTORY_ENTRY_EVENT_TYPES;
use acp_core::model::{
    AcpRaw, AgentId, AgentRef, Digest, EventKind, EventOrigin, EventPayload, EventType,
    GlobalCursor, OriginEpoch, PendingEvent, PortError, RawUnavailableReason, Sequence,
    ServerEpoch, SessionId, StoredPolicy, Timestamp, ViewJson,
};
use acp_core::ports::{
    HistoryInclude, HistoryPage, HistoryQuery, NewSession, OwnedCommit, OwnedEventRecord,
    ReplayLimit, RetentionPolicy, SessionStore, StateChange,
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

fn epoch() -> OriginEpoch {
    OriginEpoch::new(EPOCH).expect("epoch")
}

fn event(
    kind: EventKind,
    event_type: &str,
    policy: StoredPolicy,
    view: &str,
    acp: Option<AcpRaw>,
) -> PendingEvent {
    PendingEvent::new(
        kind,
        EventType::new(event_type).expect("event type"),
        policy,
        EventPayload::new(ViewJson::new(view).expect("view"), acp),
        EventOrigin::Agent,
        None,
        None,
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
                title: Some("history".to_owned()),
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

fn append(session: &SessionId, hour: u32, events: Vec<PendingEvent>) -> OwnedCommit {
    OwnedCommit {
        session: Some(session.clone()),
        at: at(hour),
        expected_version: None,
        state: None,
        turns: Vec::new(),
        events,
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: None,
        command_terminal: None,
        origin_epoch: Some(epoch()),
    }
}

/// 只开 `history` 的历史页查询：`messages` 关掉，用来固定「两个集合互不影响」。
fn query(session: &SessionId, before: Option<u64>, limit: u32) -> HistoryQuery {
    HistoryQuery {
        session: session.clone(),
        include: HistoryInclude {
            messages: false,
            turns: false,
            pending_interactions: false,
            config_options: false,
            capabilities: false,
            history: true,
        },
        before_history: before.map(|value| Sequence::new(value).expect("cursor")),
        before_messages: None,
        messages_limit: ReplayLimit::new(0),
        after: None,
        limit: ReplayLimit::new(limit),
    }
}

/// 同时开 `messages` 与 `history` 的查询：两个面各自带游标，用来固定「各自分页、互不影响」。
fn both_faces(
    session: &SessionId,
    after: Option<u64>,
    before_history: Option<u64>,
    limit: u32,
) -> HistoryQuery {
    HistoryQuery {
        session: session.clone(),
        include: HistoryInclude {
            messages: true,
            turns: false,
            pending_interactions: false,
            config_options: false,
            capabilities: false,
            history: true,
        },
        before_history: before_history.map(|value| Sequence::new(value).expect("cursor")),
        before_messages: None,
        messages_limit: ReplayLimit::new(0),
        after: after.map(cursor),
        limit: ReplayLimit::new(limit),
    }
}

/// `messages` 面的全局游标。`read_session` 只比较其 `global_sequence`（epoch 一致性由 `replay` 判定）。
fn cursor(sequence: u64) -> GlobalCursor {
    GlobalCursor::new(
        ServerEpoch::new(EPOCH).expect("server epoch"),
        Sequence::new(sequence).expect("cursor"),
    )
}

/// `SYNC_PROTOCOL.md` §10.2.1 的第一张表（逐条照抄）：进入会话结构化历史条目的 17 类。
const ENTERING: &[&str] = &[
    "turn.queued",
    "turn.started",
    "turn.completed",
    "turn.cancelled",
    "turn.failed",
    "tool.call.started",
    "tool.call.updated",
    "tool.call.completed",
    "permission.requested",
    "permission.resolved",
    "elicitation.requested",
    "elicitation.resolved",
    "terminal.output",
    "file.changed",
    "session.plan.changed",
    "session.mode.changed",
    "session.config.changed",
];

/// §10.2.1 的第二张表（逐条照抄）：不进入历史条目的 17 类。
const EXCLUDED: &[&str] = &[
    "user.message.delta",
    "agent.message.delta",
    "agent.thought.delta",
    "agent.message.completed",
    "session.created",
    "session.updated",
    "session.info.changed",
    "session.usage.changed",
    "session.commands.changed",
    "session.origin.online_changed",
    "command.completed",
    "command.failed",
    "command.uncertain",
    "agent.connected",
    "agent.disconnected",
    "device.revoked",
    "turn.delta_compacted",
];

/// 按事件类型造一条事件。历史页只按 `event_type` 筛选，`kind`/`policy`（保留与压缩维度）因此取该类型
/// 在生产里的口径：消息增量是 `delta` + 短保留，收尾正文是 `final_message`，`turn.delta_compacted`
/// 是 `summary`，其余按结构化事件。
fn typed_event(event_type: &str) -> PendingEvent {
    let (kind, policy) = match event_type {
        "user.message.delta" | "agent.message.delta" | "agent.thought.delta" => {
            (EventKind::Delta, StoredPolicy::ShortTerm)
        }
        "agent.message.completed" => (EventKind::FinalMessage, StoredPolicy::Durable),
        "turn.delta_compacted" => (EventKind::Summary, StoredPolicy::Durable),
        _ => (EventKind::Structured, StoredPolicy::Durable),
    };
    event(
        kind,
        event_type,
        policy,
        &format!(r#"{{"probe":"{event_type}"}}"#),
        None,
    )
}

fn types(records: &[OwnedEventRecord]) -> Vec<&str> {
    records
        .iter()
        .map(|record| record.event.event_type.as_str())
        .collect()
}

fn history_types(page: &HistoryPage) -> Vec<&str> {
    types(&page.history)
}

fn event_types(page: &HistoryPage) -> Vec<&str> {
    types(&page.events)
}

fn event_sequences(page: &HistoryPage) -> Vec<u64> {
    page.events
        .iter()
        .map(|record| record.event.global_sequence.get())
        .collect()
}

fn sequences(page: &HistoryPage) -> Vec<u64> {
    page.history
        .iter()
        .map(|record| {
            record
                .event
                .session_sequence
                .expect("每条条目都必须带会话内序号")
                .get()
        })
        .collect()
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

/// §9 判据 33 ①②③⑤：一次取行连正文、按会话内序号分页不重不漏、同一游标同结果、原文不可用带原因。
#[tokio::test]
async fn history_pages_carry_bodies_and_never_overlap() {
    let dir = temp_dir("history-paging");
    let store = open(&dir).await;
    let session = create_session(&store).await;

    let acp_digest = Digest::new(&digest_text("history-acp")).expect("digest");
    let available_raw = r#"{"path":"src/a.rs","addedLines":3}"#;
    let cleaned_digest = Digest::new(&digest_text("history-acp-cleaned")).expect("digest");

    // 一次提交写 5 条事件：第 1 条原文可用、第 4 条按保留期不可用。
    store
        .commit(append(
            &session,
            1,
            vec![
                event(
                    EventKind::Structured,
                    "tool.call.started",
                    StoredPolicy::Durable,
                    r#"{"toolCallId":"c1"}"#,
                    Some(
                        AcpRaw::available(
                            "application/vnd.acp.tool+json",
                            available_raw,
                            acp_digest.clone(),
                        )
                        .expect("acp available"),
                    ),
                ),
                event(
                    EventKind::Structured,
                    "tool.call.updated",
                    StoredPolicy::Durable,
                    r#"{"toolCallId":"c1","update":1}"#,
                    None,
                ),
                event(
                    EventKind::Structured,
                    "terminal.output",
                    StoredPolicy::Durable,
                    r#"{"chunk":"ok"}"#,
                    None,
                ),
                event(
                    EventKind::Structured,
                    "file.changed",
                    StoredPolicy::Durable,
                    r#"{"changeId":"f1"}"#,
                    Some(AcpRaw::unavailable(
                        RawUnavailableReason::RetentionExpired,
                        17,
                        Some(cleaned_digest.clone()),
                    )),
                ),
                event(
                    EventKind::Structured,
                    "session.plan.changed",
                    StoredPolicy::Durable,
                    r#"{"plan":[1]}"#,
                    None,
                ),
            ],
        ))
        .await
        .expect("append events");

    let view = store.read_view().await.expect("read view");

    // 省略游标 = 最新一页（limit 2），页内按会话内序号升序。
    let newest = view
        .read_session(query(&session, None, 2))
        .await
        .expect("newest page");
    assert_eq!(sequences(&newest), vec![4, 5], "省略游标必须给最新一页");
    assert!(newest.history_has_earlier, "更早的条目仍在 → 页标记为真");
    assert!(
        newest.events.is_empty(),
        "messages 面仍只受 include.messages 控制（这里是关的）"
    );

    // 带游标续读：紧接该游标之前，不与上一页重叠、也不跳过。
    let middle = view
        .read_session(query(&session, Some(4), 2))
        .await
        .expect("middle page");
    assert_eq!(sequences(&middle), vec![2, 3]);
    assert!(middle.history_has_earlier);

    let oldest = view
        .read_session(query(&session, Some(2), 2))
        .await
        .expect("oldest page");
    assert_eq!(sequences(&oldest), vec![1]);
    assert!(!oldest.history_has_earlier, "到最早一条时页标记为假");

    let mut all = sequences(&newest);
    all.extend(sequences(&middle));
    all.extend(sequences(&oldest));
    all.sort_unstable();
    assert_eq!(all, vec![1, 2, 3, 4, 5], "三页必须不重不漏");

    // 同一游标 + 同一 limit 重复读 → 逐字段相同（含正文）。
    let repeated = view
        .read_session(query(&session, Some(4), 2))
        .await
        .expect("repeat");
    assert_eq!(repeated, middle, "同一游标重复读必须给出同一序列");

    // 以仍可读的最早一条为游标：空页 + 页标记为假，且不是错误。
    let done = view
        .read_session(query(&session, Some(1), 2))
        .await
        .expect("earliest cursor");
    assert!(done.history.is_empty(), "已到最早一条时必须不返回空占位");
    assert!(!done.history_has_earlier);
    drop(view);

    // 正文与定位来自同一条行：`view` 与库内 `payload_json` 逐字节相同，ACP 原文保真。
    let first = &oldest.history[0];
    let (media_type, raw_json, byte_length, sha256) = first
        .payload
        .acp
        .as_ref()
        .expect("原文可用的条目必须带 ACP")
        .as_available()
        .expect("available");
    assert_eq!(media_type, "application/vnd.acp.tool+json");
    assert_eq!(raw_json, available_raw, "原文必须字节保真");
    assert_eq!(byte_length, available_raw.len() as u64);
    assert_eq!(sha256, &acp_digest);

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let stored_view: String = sqlx::query_scalar(
        "SELECT payload_json FROM owned_event WHERE session_id = ?1 AND session_sequence = 1",
    )
    .bind(session.as_str())
    .fetch_one(&pool)
    .await
    .expect("stored row");
    assert_eq!(
        first.payload.view.as_str(),
        stored_view.as_str(),
        "view 必须是库内那串字节（不重新序列化）"
    );
    pool.close().await;

    // 原文不可用的行按已登记原因还原，且摘要不因原文被清掉而丢失。
    let cleaned = &newest.history[0];
    assert_eq!(cleaned.event.session_sequence.expect("sequence").get(), 4);
    let (reason, byte_length, sha256) = cleaned
        .payload
        .acp
        .as_ref()
        .expect("不可用原因必须在页里")
        .as_unavailable()
        .expect("unavailable");
    assert_eq!(reason, RawUnavailableReason::RetentionExpired);
    assert_eq!(byte_length, 17);
    assert_eq!(sha256, Some(&cleaned_digest));

    store.close().await;
}

/// §9 判据 33 ④：已压缩的行不产生条目，也不占位、不计入页标记；压缩的收据事件（`turn.delta_compacted`）
/// 本身也不是条目（§10.2.1 把它登记在「不进入」一侧）。
#[tokio::test]
async fn compacted_rows_never_become_history_entries() {
    let dir = temp_dir("history-compacted");
    let store = open(&dir).await;
    let session = create_session(&store).await;

    // 1 条结构化事件 + 3 条 delta（delta 随后被折进 summary 事件）。
    let written = store
        .commit(append(
            &session,
            1,
            vec![
                event(
                    EventKind::Structured,
                    "tool.call.started",
                    StoredPolicy::Durable,
                    r#"{"toolCallId":"c1"}"#,
                    None,
                ),
                event(
                    EventKind::Delta,
                    "agent.message.delta",
                    StoredPolicy::ShortTerm,
                    r#"{"deltaIndex":0}"#,
                    None,
                ),
                event(
                    EventKind::Delta,
                    "agent.message.delta",
                    StoredPolicy::ShortTerm,
                    r#"{"deltaIndex":1}"#,
                    None,
                ),
                event(
                    EventKind::Delta,
                    "agent.message.delta",
                    StoredPolicy::ShortTerm,
                    r#"{"deltaIndex":2}"#,
                    None,
                ),
            ],
        ))
        .await
        .expect("append events");
    let compacted: Vec<GlobalCursor> = {
        let server_epoch =
            acp_core::model::ServerEpoch::new(&store_epoch(&dir).await).expect("server epoch");
        written.appended[1..]
            .iter()
            .map(|event| GlobalCursor::new(server_epoch.clone(), event.global_sequence))
            .collect()
    };

    let mut compaction = append(
        &session,
        2,
        vec![event(
            EventKind::Summary,
            "turn.delta_compacted",
            StoredPolicy::Durable,
            r#"{"turnId":"33333333-3333-4333-8333-333333333333","deltaCount":3}"#,
            None,
        )],
    );
    compaction.compacted = compacted;
    store.commit(compaction).await.expect("compaction");

    let view = store.read_view().await.expect("read view");
    let page = view
        .read_session(query(&session, None, 10))
        .await
        .expect("history page");
    assert_eq!(
        sequences(&page),
        vec![1],
        "被压缩的三行与压缩收据（`turn.delta_compacted`）都不产生条目，也不以占位条目出现"
    );
    assert!(!page.history_has_earlier);

    // 游标落在被压缩的区间内、而更早仍有可读条目 → 正常分页，不报保留期错误。
    let older = view
        .read_session(query(&session, Some(4), 10))
        .await
        .expect("cursor inside the compacted range");
    assert_eq!(sequences(&older), vec![1]);
    assert!(!older.history_has_earlier);

    drop(view);
    store.close().await;
}

/// §9 判据 33 ④ 的**列级**守卫：`compacted_into IS NOT NULL` 得继续把该行排除——即使它的 `event_type`
/// 在 §10.2.1 的条目集合内（所以类型谓词排不掉它）。压缩的 `UPDATE` 只接受 `kind = 'delta'` 的行，
/// 因此这里刻意用「`kind = 'delta'` + 条目类型」造出这一组合（列级守卫不能靠「delta 都不是条目」这条
/// 类型口径代偿）。删掉 `compacted_into IS NULL`、只留类型谓词时本用例变红。
#[tokio::test]
async fn compacted_flag_excludes_a_row_the_type_predicate_would_keep() {
    let dir = temp_dir("history-compacted-column");
    let store = open(&dir).await;
    let session = create_session(&store).await;

    let written = store
        .commit(append(
            &session,
            1,
            vec![
                event(
                    EventKind::Structured,
                    "tool.call.started",
                    StoredPolicy::Durable,
                    r#"{"toolCallId":"c1"}"#,
                    None,
                ),
                event(
                    EventKind::Delta,
                    "terminal.output",
                    StoredPolicy::ShortTerm,
                    r#"{"chunk":"late"}"#,
                    None,
                ),
                event(
                    EventKind::State,
                    "turn.completed",
                    StoredPolicy::Durable,
                    r#"{"state":"completed"}"#,
                    None,
                ),
            ],
        ))
        .await
        .expect("append events");
    let server_epoch =
        acp_core::model::ServerEpoch::new(&store_epoch(&dir).await).expect("server epoch");
    let mut compaction = append(
        &session,
        2,
        vec![event(
            EventKind::Summary,
            "turn.delta_compacted",
            StoredPolicy::Durable,
            r#"{"turnId":"33333333-3333-4333-8333-333333333333","deltaCount":1}"#,
            None,
        )],
    );
    compaction.compacted = vec![GlobalCursor::new(
        server_epoch,
        written.appended[1].global_sequence,
    )];
    store.commit(compaction).await.expect("compaction");

    // 先确认夹具真的落在那个状态（否则本用例会静默变成恒真）。
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let flag: Option<i64> = sqlx::query_scalar(
        "SELECT compacted_into FROM owned_event WHERE session_id = ?1 AND session_sequence = 2",
    )
    .bind(session.as_str())
    .fetch_one(&pool)
    .await
    .expect("stored row");
    assert_eq!(flag, Some(4), "序号 2 的行必须已指向压缩收据行");
    pool.close().await;

    let view = store.read_view().await.expect("read view");
    let page = view
        .read_session(query(&session, None, 10))
        .await
        .expect("history page");
    assert_eq!(
        sequences(&page),
        vec![1, 3],
        "被标记为已压缩的行即使类型在条目集合内也不得成为条目，也不占位"
    );
    assert!(!page.history_has_earlier);

    let cursor_on_it = view
        .read_session(query(&session, Some(3), 10))
        .await
        .expect("cursor before the compacted row");
    assert_eq!(sequences(&cursor_on_it), vec![1]);

    drop(view);
    store.close().await;
}

/// §10.2.1 是条目集合的唯一来源：core 与 `storage-sqlite` 共用的 `HISTORY_ENTRY_EVENT_TYPES` 必须与文档
/// 的两张表逐条相等（进入的 17 类一条不少、一条不多），且登记为「不进入」的 17 类一个都不在内。
///
/// 变红时机：常量漏写/多写一类、把某个不进入的类型混入集合、或改了 §10.2.1 的两张表却没改常量
/// （下面两张表逐条照抄文档，文档变了这里不动就说明改动没同步）。
#[test]
fn history_entry_types_match_the_documented_sets() {
    assert_eq!(
        HISTORY_ENTRY_EVENT_TYPES, ENTERING,
        "条目集合必须逐条等于 §10.2.1 的 17 类"
    );
    assert_eq!(HISTORY_ENTRY_EVENT_TYPES.len(), 17);
    assert_eq!(EXCLUDED.len(), 17);
    for excluded in EXCLUDED {
        assert!(
            !HISTORY_ENTRY_EVENT_TYPES.contains(excluded),
            "{excluded} 登记在「不进入历史条目」一侧，不得出现在集合里"
        );
    }
    for entering in ENTERING {
        assert!(
            !EXCLUDED.contains(entering),
            "{entering} 不得同时出现在两张表里"
        );
    }
}

/// §10.2.1 的两张表落到查询上：17 类进条目的事件与 17 类不进的事件**交替**落盘（17 + 17 = 34 行），历史页
/// 只给条目集合内的类型，且筛选与 `limit`/页标记在**同一条语句**里生效——非条目行既不出现在页里，也不占
/// 页大小、不计入页标记。事后过滤会让 `limit + 1` 的窗口落在非条目行上，返回空页或短页。
#[tokio::test]
async fn history_entries_are_filtered_by_type_within_the_page_query() {
    let dir = temp_dir("history-entry-types");
    let store = open(&dir).await;
    let session = create_session(&store).await;

    // 奇数序号 = 条目（`ENTERING[i]`）、偶数序号 = 非条目（`EXCLUDED[i]`）：因此尾部窗口里条目稀疏。
    let mut events = Vec::new();
    for (entering, excluded) in ENTERING.iter().zip(EXCLUDED.iter()) {
        events.push(typed_event(entering));
        events.push(typed_event(excluded));
    }
    assert_eq!(events.len(), 34);
    store
        .commit(append(&session, 1, events))
        .await
        .expect("append events");

    let view = store.read_view().await.expect("read view");

    // 全量一页：条目恰好是那 17 类，按会话内序号升序，页标记为假。
    let all = view
        .read_session(query(&session, None, 40))
        .await
        .expect("full page");
    assert_eq!(history_types(&all), ENTERING.to_vec());
    assert_eq!(
        sequences(&all),
        (1..=33).step_by(2).collect::<Vec<u64>>(),
        "条目来自奇数序号的行（偶数序号全是非条目）"
    );
    assert!(!all.history_has_earlier);

    // 省略游标 + limit 3：`limit + 1` 的窗口覆盖序号 29..34，其中只有 29/31/33 是条目——页必须满 3 条，
    // 且页标记为真（事后过滤会给出 2 条甚至空页）。
    let newest = view
        .read_session(query(&session, None, 3))
        .await
        .expect("newest page");
    assert_eq!(sequences(&newest), vec![29, 31, 33], "非条目行不得占页大小");
    assert_eq!(history_types(&newest), ENTERING[14..].to_vec());
    assert!(newest.history_has_earlier);

    // 游标落在**非条目**事件上（32 = `device.revoked`）：页大小、页标记与可见条目仍按条目计数，也不报
    // 保留期类错误。
    let inside = view
        .read_session(query(&session, Some(32), 3))
        .await
        .expect("cursor on a non-entry row");
    assert_eq!(sequences(&inside), vec![27, 29, 31]);
    assert_eq!(history_types(&inside), ENTERING[13..16].to_vec());
    assert!(inside.history_has_earlier);

    // 非条目行仍完整出现在 `messages` 面的原料里：筛选只作用于 `history` 集合。
    let messages = view
        .read_session(both_faces(&session, None, None, 40))
        .await
        .expect("messages page");
    assert_eq!(
        event_sequences(&messages).len(),
        34,
        "`messages` 面给出全部可读事件，不做条目筛选"
    );
    assert!(event_types(&messages).contains(&"agent.message.completed"));
    assert!(!history_types(&messages).contains(&"agent.message.completed"));

    drop(view);
    store.close().await;
}

/// §9 判据 33 ② 与 Δspec 的「两者同时请求时各自分页」：同一请求里 `messages` 与 `history` 各自分页，任一
/// 方的游标都不改变另一方的返回内容与页边界；`next` 只服务 `messages` 面，与 `history` 开关无关。
#[tokio::test]
async fn messages_and_history_page_independently_in_one_request() {
    let dir = temp_dir("history-both-faces");
    let store = open(&dir).await;
    let session = create_session(&store).await;

    // 条目（4/5/7）与非条目（1/2/3/6）混排，两个面都能看出各自的页边界。
    store
        .commit(append(
            &session,
            1,
            vec![
                typed_event("user.message.delta"),
                typed_event("agent.message.delta"),
                typed_event("agent.message.completed"),
                typed_event("tool.call.started"),
                typed_event("terminal.output"),
                typed_event("command.completed"),
                typed_event("turn.completed"),
            ],
        ))
        .await
        .expect("append events");

    let view = store.read_view().await.expect("read view");

    // 两个游标各给一半：`after` 走全局序号、`before_history` 走会话内序号。
    let both = view
        .read_session(both_faces(&session, Some(2), Some(7), 2))
        .await
        .expect("both faces");
    assert_eq!(
        event_types(&both),
        vec!["agent.message.completed", "tool.call.started"],
        "`events` 只受 after/limit 影响"
    );
    assert_eq!(event_sequences(&both), vec![3, 4]);
    assert_eq!(
        both.next
            .as_ref()
            .map(|cursor| cursor.global_sequence.get()),
        Some(4),
        "`next` 是 messages 面的续读游标"
    );
    assert_eq!(
        sequences(&both),
        vec![4, 5],
        "`history` 只受 before_history/limit 影响"
    );
    assert!(!both.history_has_earlier);

    // 换掉历史游标：`events`/`next` 逐字段不变，历史分页照常移动。
    let latest_history = view
        .read_session(both_faces(&session, Some(2), None, 2))
        .await
        .expect("both faces");
    assert_eq!(latest_history.events, both.events);
    assert_eq!(
        latest_history.next, both.next,
        "history 游标不得改变 messages 面的页边界"
    );
    assert_eq!(sequences(&latest_history), vec![5, 7]);
    assert!(latest_history.history_has_earlier);

    // 换掉消息游标：历史页与页标记逐字段不变。
    let from_start = view
        .read_session(both_faces(&session, None, Some(7), 2))
        .await
        .expect("both faces");
    assert_eq!(event_sequences(&from_start), vec![1, 2]);
    assert_eq!(
        event_types(&from_start),
        vec!["user.message.delta", "agent.message.delta"]
    );
    assert_eq!(from_start.history, both.history);
    assert_eq!(from_start.history_has_earlier, both.history_has_earlier);

    drop(view);
    store.close().await;
}

/// §9 判据 33 ⑥：游标早于该会话仍可读条目的下界（更早的行已按保留策略清理）→ 显式报错。
#[tokio::test]
async fn a_cursor_before_the_retained_window_is_rejected() {
    let dir = temp_dir("history-window");
    let store = open(&dir).await;
    let session = create_session(&store).await;

    // 序号 1 = delta（7 天窗口）、序号 2 = 结构化（90 天窗口）。
    store
        .commit(append(
            &session,
            1,
            vec![
                event(
                    EventKind::Delta,
                    "agent.message.delta",
                    StoredPolicy::ShortTerm,
                    r#"{"deltaIndex":0}"#,
                    None,
                ),
                event(
                    EventKind::Structured,
                    "tool.call.started",
                    StoredPolicy::Durable,
                    r#"{"toolCallId":"c1"}"#,
                    None,
                ),
            ],
        ))
        .await
        .expect("append events");

    // 8 天后：delta 到期被清理，窗口下界前移到序号 2。
    let report = store
        .prune(
            RetentionPolicy {
                transcript_retention_days: 90,
                sync_event_retention_days: 7,
                audit_retention_days: 365,
                max_total_size_bytes: 2 * 1024 * 1024 * 1024,
                max_session_size_bytes: 100 * 1024 * 1024,
                persist_deltas: false,
            },
            Timestamp::new("2026-09-26T00:00:00.000Z").expect("timestamp"),
        )
        .await
        .expect("prune");
    assert_eq!(report.removed_events, 1, "只有过期的 delta 被清理");

    let view = store.read_view().await.expect("read view");

    // 无游标：仍可读的最早一条，页标记为假（它自己是下界，不是「已到 0」）。
    let page = view
        .read_session(query(&session, None, 10))
        .await
        .expect("history page");
    assert_eq!(sequences(&page), vec![2]);
    assert!(!page.history_has_earlier);

    // 以仍可读的最早一条为游标：空页 + 页标记为假（不返回空占位）。
    let at_boundary = view
        .read_session(query(&session, Some(2), 10))
        .await
        .expect("boundary cursor");
    assert!(at_boundary.history.is_empty());
    assert!(!at_boundary.history_has_earlier);

    // 游标早于下界（序号 1 已被清理）→ 保留期类错误，不静默返回空页。
    let expired = view
        .read_session(query(&session, Some(1), 10))
        .await
        .expect_err("游标早于保留窗口必须失败");
    assert!(
        matches!(expired, PortError::InvalidRequest(_)),
        "必须是既有的端口错误变体：{expired:?}"
    );

    drop(view);
    store.close().await;
}
