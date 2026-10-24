//! 消息正文页：`(createdAt, messageId)` 复合游标、每页上限与 `messagesHasEarlier`。
//!
//! `HistoryPage.messages` 不能整段载入：实现按 `messages_limit` 取一页，媒体引用也只对同一批消息查询。

mod support;

use acp_core::model::{AgentId, AgentRef, MessageId, OriginEpoch, SessionId, Timestamp};
use acp_core::ports::{
    AttachmentStore, HistoryInclude, HistoryQuery, MessageCursor, NewSession, OwnedCommit,
    ReplayLimit, SessionStore, StateChange,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";

fn at(hour: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{hour:02}:00:00.000Z")).expect("timestamp")
}

fn message_id(n: u64) -> MessageId {
    MessageId::new(&format!("00000000-0000-4000-8000-{n:012}")).expect("message id")
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
                title: Some("messages".to_owned()),
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
            origin_epoch: Some(OriginEpoch::new(EPOCH).expect("epoch")),
            input: None,
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id")
}

async fn insert_message(
    dir: &std::path::Path,
    session: &SessionId,
    n: u64,
    hour: u32,
) -> MessageId {
    let id = message_id(n);
    let pool = raw_write_pool(&dir.join("acp-remote.sqlite3")).await;
    sqlx::query(
        "INSERT INTO owned_message (message_id, session_id, turn_id, channel, state, \
         content_json, partial_reason, created_at, updated_at) \
         VALUES (?1, ?2, NULL, 'reply', 'completed', '[]', NULL, ?3, ?3)",
    )
    .bind(id.as_str())
    .bind(session.as_str())
    .bind(at(hour).as_str())
    .execute(&pool)
    .await
    .expect("message");
    pool.close().await;
    id
}

fn page_query(session: &SessionId, before: Option<MessageCursor>, limit: u32) -> HistoryQuery {
    HistoryQuery {
        origin_order: false,
        session: session.clone(),
        include: HistoryInclude {
            messages: true,
            ..HistoryInclude::default()
        },
        before_history: None,
        before_messages: before,
        messages_limit: ReplayLimit::new(limit),
        after: None,
        limit: ReplayLimit::new(0),
    }
}

/// 三页覆盖五条消息：不重不漏、升序、页标记逐页收敛；第二条游标页仍可续读到最早一条。
#[tokio::test]
async fn message_pages_use_the_composite_cursor_without_gaps_or_duplicates() {
    let dir = temp_dir("message-pages");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    let mut ids = Vec::new();
    for n in 1..=5 {
        ids.push(insert_message(&dir, &session, n, n as u32).await);
    }

    let view = store.read_view().await.expect("view");
    let first = view
        .read_session(page_query(&session, None, 2))
        .await
        .expect("first page");
    assert_eq!(
        first
            .messages
            .iter()
            .map(|message| &message.message)
            .cloned()
            .collect::<Vec<_>>(),
        vec![ids[3].clone(), ids[4].clone()],
        "省略游标 = 最新一页，按 (createdAt, messageId) 升序"
    );
    assert!(first.messages_has_earlier);

    let cursor = MessageCursor {
        sequence: None,
        created_at: first.messages[0].created_at.clone(),
        message_id: first.messages[0].message.clone(),
    };
    let second = view
        .read_session(page_query(&session, Some(cursor), 2))
        .await
        .expect("second page");
    assert_eq!(
        second
            .messages
            .iter()
            .map(|message| &message.message)
            .cloned()
            .collect::<Vec<_>>(),
        vec![ids[1].clone(), ids[2].clone()]
    );
    assert!(second.messages_has_earlier);

    let cursor = MessageCursor {
        sequence: None,
        created_at: second.messages[0].created_at.clone(),
        message_id: second.messages[0].message.clone(),
    };
    let third = view
        .read_session(page_query(&session, Some(cursor), 2))
        .await
        .expect("third page");
    assert_eq!(
        third
            .messages
            .iter()
            .map(|message| &message.message)
            .cloned()
            .collect::<Vec<_>>(),
        vec![ids[0].clone()]
    );
    assert!(!third.messages_has_earlier, "已到最早一条");

    // 续读不重不漏：从最早一页拼回来正好是全部消息的升序集合。
    let mut seen: Vec<MessageId> = Vec::new();
    seen.extend(third.messages.iter().map(|m| &m.message).cloned());
    seen.extend(second.messages.iter().map(|m| &m.message).cloned());
    seen.extend(first.messages.iter().map(|m| &m.message).cloned());
    assert_eq!(seen, ids);

    // `messages_limit = 0`：明确不取正文，页标记恒为假（不能据此判断「没有更早」）。
    let none = view
        .read_session(page_query(&session, None, 0))
        .await
        .expect("empty page");
    assert!(none.messages.is_empty());
    assert!(!none.messages_has_earlier);
    drop(view);
    store.close().await;
}

/// 媒体引用只按本页消息取：翻到不含媒体的页时不返回任何引用，也不做整会话扫描。
#[tokio::test]
async fn media_references_are_scoped_to_the_returned_message_page() {
    let dir = temp_dir("message-pages-media");
    let store = open(&dir).await;
    let session = create_session(&store).await;
    let ids: Vec<MessageId> = {
        let mut ids = Vec::new();
        for n in 1..=3 {
            ids.push(insert_message(&dir, &session, n, n as u32).await);
        }
        ids
    };
    let attachment = store
        .put(b"\x89PNG", "image/png", at(10))
        .await
        .expect("put");
    let pool = raw_write_pool(&dir.join("acp-remote.sqlite3")).await;
    sqlx::query(
        "INSERT INTO owned_media_ref (message_id, position, attachment_id, kind) \
         VALUES (?1, 0, ?2, 'image')",
    )
    .bind(ids[2].as_str())
    .bind(attachment.id.as_str())
    .execute(&pool)
    .await
    .expect("media ref");
    pool.close().await;

    let view = store.read_view().await.expect("view");
    let latest = view
        .read_session(page_query(&session, None, 1))
        .await
        .expect("latest");
    assert_eq!(latest.messages.len(), 1);
    assert_eq!(latest.messages[0].media.len(), 1, "本页消息带媒体");
    assert_eq!(latest.messages[0].media[0].attachment(), &attachment.id);

    let cursor = MessageCursor {
        sequence: None,
        created_at: latest.messages[0].created_at.clone(),
        message_id: latest.messages[0].message.clone(),
    };
    let older = view
        .read_session(page_query(&session, Some(cursor), 2))
        .await
        .expect("older");
    assert_eq!(older.messages.len(), 2);
    assert!(
        older
            .messages
            .iter()
            .all(|message| message.media.is_empty()),
        "其它页不得携带本页之外的媒体引用"
    );
    drop(view);
    store.close().await;
}
