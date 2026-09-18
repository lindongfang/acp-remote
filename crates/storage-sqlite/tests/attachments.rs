//! §5.3/§7.3：附件的内容寻址写入、`link` 与 LRU 清理。

mod support;

use acp_core::model::{AttachmentGeneration, Timestamp};
use acp_core::ports::{AttachmentStore, SessionStore, StateChange};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

fn at(hour: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{hour:02}:00:00.000Z")).expect("timestamp")
}

async fn store(dir: &std::path::Path, budget: u64) -> SqliteStore {
    let mut config = StorageConfig::new(dir);
    config.attachment_max_total_bytes = budget;
    SqliteStore::open(config, &at(0)).await.expect("open")
}

async fn session(store: &SqliteStore) -> acp_core::model::SessionId {
    store
        .commit(acp_core::ports::OwnedCommit {
            session: None,
            at: at(0),
            expected_version: None,
            state: Some(StateChange::Create(acp_core::ports::NewSession {
                title: Some("attachments".to_owned()),
                agent: acp_core::model::AgentRef::try_new(
                    acp_core::model::AgentId::new("probe-agent").expect("agent"),
                    "Probe Agent",
                )
                .expect("agent ref"),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(
                acp_core::model::OriginEpoch::new("6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b")
                    .expect("epoch"),
            ),
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id")
}

/// 内容寻址：同样的字节只落一份文件与一行元数据，`get` 原样返回。
#[tokio::test]
async fn attachments_are_content_addressed() {
    let dir = temp_dir("attachments-addressed");
    let store = store(&dir, 1024 * 1024).await;
    let bytes = b"attachment-bytes".to_vec();

    let first = store.put(&bytes, "text/plain", at(1)).await.expect("put");
    assert_eq!(first.byte_length, bytes.len() as u64);
    assert_eq!(first.media_type, "text/plain");
    assert!(first.relative_path.starts_with("attachments/"));
    assert_eq!(
        std::fs::read(dir.join(&first.relative_path)).expect("file"),
        bytes
    );

    let again = store.put(&bytes, "text/plain", at(2)).await.expect("put");
    assert_eq!(again.id, first.id, "same content, same id");
    assert_eq!(again.sha256, first.sha256);

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        1
    );
    pool.close().await;

    // 未 link 的内容也能按 id 取回（id 由内容地址确定性导出）。
    let fetched = store.get(&first.id).await.expect("get").expect("bytes");
    assert_eq!(fetched, bytes);
    assert!(
        store
            .get(
                &acp_core::model::AttachmentId::new("99999999-9999-4999-8999-999999999999")
                    .expect("id")
            )
            .await
            .expect("get")
            .is_none()
    );
    store.close().await;
}

/// 超出单文件上限 → 显式拒绝。
#[tokio::test]
async fn oversized_attachment_is_rejected() {
    let dir = temp_dir("attachments-limit");
    let mut config = StorageConfig::new(&dir);
    config.attachment_max_file_bytes = 8;
    let store = SqliteStore::open(config, &at(0)).await.expect("open");
    let error = store
        .put(b"0123456789", "text/plain", at(1))
        .await
        .expect_err("too large");
    assert!(matches!(
        error,
        acp_core::model::PortError::InvalidRequest(_)
    ));
    assert!(
        !dir.join("attachments")
            .read_dir()
            .expect("dir")
            .any(|entry| {
                entry
                    .map(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
                    .unwrap_or(false)
            })
    );
    store.close().await;
}

/// `link` 记录会话归属与 generation，并对未知会话/附件给出具名错误。
#[tokio::test]
async fn link_records_session_ownership() {
    let dir = temp_dir("attachments-link");
    let store = store(&dir, 1024 * 1024).await;
    let session = session(&store).await;
    let reference = store
        .put(b"linked", "text/plain", at(1))
        .await
        .expect("put");

    store
        .link(&session, &reference.id, AttachmentGeneration::new(3))
        .await
        .expect("link");
    store
        .link(&session, &reference.id, AttachmentGeneration::new(4))
        .await
        .expect("re-link bumps generation");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT generation FROM owned_attachment_link").await,
        4
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment_link").await,
        1
    );
    pool.close().await;

    let unknown_session = acp_core::model::SessionId::new("99999999-9999-4999-8999-999999999999")
        .expect("session id");
    let missing_session = store
        .link(
            &unknown_session,
            &reference.id,
            AttachmentGeneration::new(1),
        )
        .await
        .expect_err("unknown session");
    assert!(matches!(
        missing_session,
        acp_core::model::PortError::NotFound(_)
    ));

    let unknown_attachment =
        acp_core::model::AttachmentId::new("99999999-9999-4999-8999-999999999999").expect("id");
    let missing_attachment = store
        .link(&session, &unknown_attachment, AttachmentGeneration::new(1))
        .await
        .expect_err("unknown attachment");
    assert!(matches!(
        missing_attachment,
        acp_core::model::PortError::InvalidRequest(_)
    ));
    store.close().await;
}

/// `prune_lru` 按 `last_used_at` 从最旧删到预算以内，行与文件一起删除。
#[tokio::test]
async fn prune_lru_removes_the_oldest_attachments_first() {
    let dir = temp_dir("attachments-lru");
    let store = store(&dir, 1024 * 1024).await;
    let mut ids = Vec::new();
    for (index, hour) in [1_u32, 2, 3].into_iter().enumerate() {
        let body = vec![u8::try_from(index).expect("byte"); 64];
        let reference = store
            .put(&body, "application/octet-stream", at(hour))
            .await
            .expect("put");
        ids.push(reference);
    }

    // 预算只够两条。
    let report = store.prune_lru(128, at(4)).await.expect("prune");
    assert_eq!(report.removed_attachments, 1);
    assert_eq!(report.freed_bytes, 64);
    assert!(!report.still_over_limit);
    assert!(
        store.get(&ids[0].id).await.expect("get").is_none(),
        "the least recently used attachment goes first"
    );
    assert!(store.get(&ids[1].id).await.expect("get").is_some());
    assert!(store.get(&ids[2].id).await.expect("get").is_some());
    assert!(!dir.join(&ids[0].relative_path).exists(), "file removed");

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        2
    );
    pool.close().await;

    // 预算为 0 → 全部清空，并报告不再超限。
    let report = store.prune_lru(0, at(5)).await.expect("prune");
    assert_eq!(report.removed_attachments, 2);
    assert!(!report.still_over_limit);
    store.close().await;
}
