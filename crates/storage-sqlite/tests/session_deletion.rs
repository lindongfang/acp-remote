mod support;

use std::path::Path;

use acp_core::model::{
    Actor, AttachmentGeneration, AttachmentId, CommandKind, CommandStatus, CommandTerminalRecord,
    ConflictKind, Digest, EventKind, EventOrigin, EventPayload, EventType, PendingEvent, PortError,
    PublicError, RequestId, ResourceGeneration, SessionDeletionBlocker, SessionDeletionEligibility,
    SessionDeletionOutcome, SessionDeletionWrite, SessionId, StoredPolicy, SubmissionId, Timestamp,
    ViewJson,
};
use acp_core::ports::{
    AttachmentRead, AttachmentStore, AttachmentTransferStore, HistoryInclude, HistoryQuery,
    IdempotencyRecord, OwnedCommit, ReplayLimit, SessionContentFacts, SessionDeletion,
    SessionQuery, SessionStore,
};
use sqlx::SqlitePool;
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::{
    AT, column_names, digest_text, meta_rows, raw_pool, raw_write_pool, scalar_i64, table_exists,
    table_snapshot, temp_dir,
};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const EPOCH: &str = "22222222-2222-4222-8222-222222222222";
const REQUEST: &str = "33333333-3333-4333-8333-333333333333";
const OTHER_REQUEST: &str = "44444444-4444-4444-8444-444444444444";
const MESSAGE: &str = "55555555-5555-4555-8555-555555555555";
const TURN: &str = "66666666-6666-4666-8666-666666666666";
const INTERACTION: &str = "77777777-7777-4777-8777-777777777777";
const ATTACHMENT: &str = "88888888-8888-4888-8888-888888888888";
const EVENT: &str = "99999999-9999-4999-8999-999999999999";
const TRANSFER: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const OTHER_TRANSFER: &str = "abababab-abab-4bab-8bab-abababababab";
const SUBMISSION: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const OTHER_SESSION: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const OTHER_EPOCH: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

fn at() -> Timestamp {
    Timestamp::new(AT).expect("timestamp")
}

fn session() -> SessionId {
    SessionId::new(SESSION).expect("session")
}

fn request(value: &str) -> RequestId {
    RequestId::new(value).expect("request")
}

fn request_fingerprint() -> Digest {
    Digest::new(&digest_text("session.delete payload")).expect("digest")
}

fn deletion_write(value: &str) -> SessionDeletionWrite {
    SessionDeletionWrite::new(
        session(),
        Actor::LocalCli,
        request(value),
        request_fingerprint(),
        at(),
    )
}

fn generation_write(value: &str, generation: u64) -> SessionDeletionWrite {
    deletion_write(value).with_expected_generation(ResourceGeneration::new(generation))
}

async fn open(dir: &Path) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at())
        .await
        .expect("open store")
}

async fn bootstrap_session_id(pool: &SqlitePool, session_id: &str, origin_epoch: &str) {
    sqlx::query(
        "INSERT INTO owned_session
           (session_id, title, agent_id, agent_name, state, origin_epoch, version,
            created_at, updated_at)
         VALUES (?1, 'Session', 'agent', 'Agent', 'idle', ?2, 1, ?3, ?3)",
    )
    .bind(session_id)
    .bind(origin_epoch)
    .bind(AT)
    .execute(pool)
    .await
    .expect("seed session");
}

async fn bootstrap_session(pool: &SqlitePool) {
    bootstrap_session_id(pool, SESSION, EPOCH).await;
}

async fn insert_delete_command(pool: &SqlitePool, request_id: &str) {
    sqlx::query(
        "INSERT INTO owned_command
           (actor_kind, actor_id, request_id, session_id, command, kind,
            expected_version, request_fingerprint, accepted_at, status)
         VALUES ('cli', 'cli', ?1, ?2, 'session.delete', 'mutation',
                 NULL, ?3, ?4, 'accepted')",
    )
    .bind(request_id)
    .bind(SESSION)
    .bind(request_fingerprint().as_str())
    .bind(AT)
    .execute(pool)
    .await
    .expect("seed accepted delete command");
}

async fn insert_other_mutation(pool: &SqlitePool, request_id: &str) {
    sqlx::query(
        "INSERT INTO owned_command
           (actor_kind, actor_id, request_id, session_id, command, kind,
            expected_version, request_fingerprint, accepted_at, status)
         VALUES ('cli', 'cli', ?1, ?2, 'session.title.set', 'mutation',
                 NULL, ?3, ?4, 'accepted')",
    )
    .bind(request_id)
    .bind(SESSION)
    .bind(digest_text("other mutation"))
    .bind(AT)
    .execute(pool)
    .await
    .expect("seed other accepted mutation");
}

async fn insert_event(pool: &SqlitePool, event_type: &str, event_id: &str, sequence: i64) {
    sqlx::query(
        "INSERT INTO owned_event
           (session_id, session_sequence, origin_epoch, origin_sequence, event_id,
            turn_id, event_type, kind, policy, origin_kind, causation, payload_json,
            payload_digest, created_at)
         VALUES (?1, ?2, ?3, ?2, ?4, NULL, ?5, 'structured', 'durable',
                 'local_cli', NULL, '{}', ?6, ?7)",
    )
    .bind(SESSION)
    .bind(sequence)
    .bind(EPOCH)
    .bind(event_id)
    .bind(event_type)
    .bind(digest_text("{}"))
    .bind(AT)
    .execute(pool)
    .await
    .expect("seed event");
}

async fn seed_content_family(pool: &SqlitePool, family: &str) {
    sqlx::raw_sql("PRAGMA foreign_keys = OFF;")
        .execute(pool)
        .await
        .expect("disable foreign keys for isolated family fixtures");
    match family {
        "owned_message" => {
            sqlx::query(
                "INSERT INTO owned_message
                   (message_id, session_id, turn_id, channel, state, content_json,
                    created_at, updated_at)
                 VALUES (?1, ?2, NULL, 'reply', 'completed',
                         '[{\"type\":\"text\",\"text\":\"content\"}]', ?3, ?3)",
            )
            .bind(MESSAGE)
            .bind(SESSION)
            .bind(AT)
            .execute(pool)
            .await
            .expect("seed message");
        }
        "owned_user_input" => {
            sqlx::query(
                "INSERT INTO owned_turn
                   (turn_id, session_id, state, queue_index, causation, started_at, ended_at)
                 VALUES (?1, ?2, 'completed', 0, NULL, ?3, ?3)",
            )
            .bind(TURN)
            .bind(SESSION)
            .bind(AT)
            .execute(pool)
            .await
            .expect("seed turn");
            sqlx::query(
                "INSERT INTO owned_message
                   (message_id, session_id, turn_id, channel, state, content_json,
                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'user', 'completed',
                         '[{\"type\":\"text\",\"text\":\"input\"}]', ?4, ?4)",
            )
            .bind(MESSAGE)
            .bind(SESSION)
            .bind(TURN)
            .bind(AT)
            .execute(pool)
            .await
            .expect("seed user message");
            sqlx::query(
                "INSERT INTO owned_user_input
                   (message_id, session_id, turn_id, actor_kind, actor_id, request_id,
                    content_digest, attachment_count, created_at)
                 VALUES (?1, ?2, ?3, 'cli', 'cli', ?4, ?5, 0, ?6)",
            )
            .bind(MESSAGE)
            .bind(SESSION)
            .bind(TURN)
            .bind(REQUEST)
            .bind(digest_text("input"))
            .bind(AT)
            .execute(pool)
            .await
            .expect("seed user input");
        }
        "owned_turn" => {
            sqlx::query(
                "INSERT INTO owned_turn
                   (turn_id, session_id, state, queue_index, causation, started_at, ended_at)
                 VALUES (?1, ?2, 'failed', 0, NULL, ?3, ?3)",
            )
            .bind(TURN)
            .bind(SESSION)
            .bind(AT)
            .execute(pool)
            .await
            .expect("seed turn");
        }
        "owned_interaction" => {
            sqlx::query(
                "INSERT INTO owned_interaction
                   (interaction_id, session_id, kind, request_event, state, created_at)
                 VALUES (?1, ?2, 'permission', NULL, 'pending', ?3)",
            )
            .bind(INTERACTION)
            .bind(SESSION)
            .bind(AT)
            .execute(pool)
            .await
            .expect("seed interaction");
        }
        "owned_media_ref" => {
            sqlx::query(
                "INSERT INTO owned_message
                   (message_id, session_id, turn_id, channel, state, content_json,
                    created_at, updated_at)
                 VALUES (?1, ?2, NULL, 'reply', 'completed', '[]', ?3, ?3)",
            )
            .bind(MESSAGE)
            .bind(SESSION)
            .bind(AT)
            .execute(pool)
            .await
            .expect("seed media owner message");
            sqlx::query(
                "INSERT INTO owned_media_ref (message_id, position, attachment_id, kind)
                 VALUES (?1, 0, ?2, 'file')",
            )
            .bind(MESSAGE)
            .bind(ATTACHMENT)
            .execute(pool)
            .await
            .expect("seed media ref");
        }
        "owned_attachment_link" => {
            insert_attachment_header(pool).await;
            sqlx::query(
                "INSERT INTO owned_attachment_link
                   (session_id, attachment_id, generation, sha256)
                 VALUES (?1, ?2, 1, ?3)",
            )
            .bind(SESSION)
            .bind(ATTACHMENT)
            .bind(digest_text("attachment bytes"))
            .execute(pool)
            .await
            .expect("seed attachment link");
        }
        "owned_timeline_anchor" => {
            sqlx::query(
                "INSERT INTO owned_timeline_anchor
                   (session_id, entity_kind, entity_id, first_sequence, first_origin_sequence)
                 VALUES (?1, 'tool', 'tool-1', 1, 1)",
            )
            .bind(SESSION)
            .execute(pool)
            .await
            .expect("seed timeline anchor");
        }
        other => panic!("unknown content family {other}"),
    }
}

async fn insert_attachment_header(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO owned_attachment
           (attachment_id, sha256, byte_length, media_type, relative_path,
            created_at, last_used_at)
         VALUES (?1, ?2, 3, 'text/plain', 'attachments/header', ?3, ?3)",
    )
    .bind(ATTACHMENT)
    .bind(digest_text("attachment bytes"))
    .bind(AT)
    .execute(pool)
    .await
    .expect("seed attachment header");
}

async fn insert_transfer(pool: &SqlitePool, transfer_id: &str) {
    sqlx::query(
        "INSERT INTO owned_attachment_transfer
           (transfer_id, session_id, subject_kind, subject_id, total_length,
            received_length, chunk_size, state, media_type, file_name,
            created_at, updated_at)
         VALUES (?1, ?2, 'device', 'device', 3, 0, 32768, 'receiving',
                 'text/plain', 'unsubmitted.txt', ?3, ?3)",
    )
    .bind(transfer_id)
    .bind(SESSION)
    .bind(AT)
    .execute(pool)
    .await
    .expect("seed transfer");
}

async fn seed_tombstoned_direct_resources(pool: &SqlitePool) {
    insert_attachment_header(pool).await;
    sqlx::query(
        "INSERT INTO owned_attachment_link
           (session_id, attachment_id, generation, sha256)
         VALUES (?1, ?2, 1, ?3)",
    )
    .bind(SESSION)
    .bind(ATTACHMENT)
    .bind(digest_text("attachment bytes"))
    .execute(pool)
    .await
    .expect("latent attachment link");
    insert_transfer(pool, TRANSFER).await;
    sqlx::query(
        "INSERT INTO owned_attachment_submission
           (submission_id, session_id, subject_kind, subject_id, state,
            attachment_count, manifest_digest, created_at, sealed_at)
         VALUES (?1, ?2, 'device', 'device', 'sealed', 1, ?3, ?4, ?4)",
    )
    .bind(SUBMISSION)
    .bind(SESSION)
    .bind(digest_text("latent manifest"))
    .bind(AT)
    .execute(pool)
    .await
    .expect("latent submission");
    sqlx::query(
        "INSERT INTO owned_attachment_submission_item
           (submission_id, position, attachment_id)
         VALUES (?1, 0, ?2)",
    )
    .bind(SUBMISSION)
    .bind(ATTACHMENT)
    .execute(pool)
    .await
    .expect("latent submission item");
    sqlx::query(
        "INSERT INTO owned_message
           (message_id, session_id, turn_id, channel, state, content_json,
            created_at, updated_at)
         VALUES (?1, ?2, NULL, 'reply', 'completed', '[]', ?3, ?3)",
    )
    .bind(MESSAGE)
    .bind(SESSION)
    .bind(AT)
    .execute(pool)
    .await
    .expect("latent media message");
    sqlx::query(
        "INSERT INTO owned_media_ref (message_id, position, attachment_id, kind)
         VALUES (?1, 0, ?2, 'file')",
    )
    .bind(MESSAGE)
    .bind(ATTACHMENT)
    .execute(pool)
    .await
    .expect("latent media reference");
}

async fn content_family_is_detected(family: &'static str) {
    let dir = temp_dir(&format!("content-{family}"));
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    seed_content_family(&pool, family).await;
    pool.close().await;

    let store = open(&dir).await;
    assert!(
        store
            .has_content(&session())
            .await
            .expect("classify content"),
        "{family} must make a session non-empty"
    );
    store.close().await;
}

#[tokio::test]
async fn every_persisted_content_family_is_classified_as_content() {
    for family in [
        "owned_message",
        "owned_user_input",
        "owned_turn",
        "owned_interaction",
        "owned_media_ref",
        "owned_timeline_anchor",
    ] {
        content_family_is_detected(family).await;
    }
}

#[tokio::test]
async fn every_content_event_type_is_conservatively_non_empty() {
    for (index, event_type) in [
        "user.message.completed",
        "user.message.delta",
        "agent.message.completed",
        "agent.thought.delta",
        "tool.call.started",
        "tool.call.updated",
        "tool.call.completed",
        "terminal.output",
        "turn.completed",
        "permission.requested",
        "elicitation.requested",
        "session.plan.changed",
        "file.changed",
        "future.content.family",
    ]
    .into_iter()
    .enumerate()
    {
        let dir = temp_dir(&format!("content-event-{index}"));
        let store = open(&dir).await;
        store.close().await;
        let path = dir.join("acp-remote.sqlite3");
        let pool = raw_write_pool(&path).await;
        bootstrap_session(&pool).await;
        insert_event(&pool, event_type, EVENT, 1).await;
        pool.close().await;

        let store = open(&dir).await;
        assert!(
            store.has_content(&session()).await.expect("classify event"),
            "{event_type} must default to content"
        );
        store.close().await;
    }
}

#[tokio::test]
async fn metadata_events_and_metadata_rows_leave_the_session_empty() {
    let metadata_events = [
        "session.created",
        "session.info.changed",
        "session.mode.changed",
        "session.config.changed",
        "session.activity.changed",
        "session.updated",
        "session.commands.changed",
        "session.usage.changed",
        "session.origin.online_changed",
        "session.deleted",
        "command.accepted",
        "command.completed",
        "command.failed",
        "command.rejected",
        "command.uncertain",
    ];
    for (index, event_type) in metadata_events.iter().enumerate() {
        let dir = temp_dir(&format!("metadata-event-{index}"));
        let store = open(&dir).await;
        store.close().await;
        let path = dir.join("acp-remote.sqlite3");
        let pool = raw_write_pool(&path).await;
        bootstrap_session(&pool).await;
        insert_event(&pool, event_type, EVENT, 1).await;
        pool.close().await;

        let store = open(&dir).await;
        assert!(
            !store
                .has_content(&session())
                .await
                .expect("classify metadata"),
            "{event_type} must remain metadata-only"
        );
        store.close().await;
    }

    let dir = temp_dir("metadata-rows");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_attachment_header(&pool).await;
    sqlx::query(
        "INSERT INTO owned_attachment_link
           (session_id, attachment_id, generation, sha256)
         VALUES (?1, ?2, 1, ?3)",
    )
    .bind(SESSION)
    .bind(ATTACHMENT)
    .bind(digest_text("attachment bytes"))
    .execute(&pool)
    .await
    .expect("seed unsubmitted attachment link");
    insert_transfer(&pool, TRANSFER).await;
    sqlx::query(
        "INSERT INTO owned_directory (directory_id, canonical_path, created_at, last_used_at)
         VALUES ('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', '/tmp/project', ?1, ?1)",
    )
    .bind(AT)
    .execute(&pool)
    .await
    .expect("seed directory metadata");
    insert_delete_command(&pool, REQUEST).await;
    pool.close().await;

    let store = open(&dir).await;
    assert!(
        !store.has_content(&session()).await.expect("metadata rows"),
        "session/header/link command/directory/unsubmitted upload are not content"
    );
    store.close().await;
}

#[tokio::test]
async fn unsubmitted_attachment_link_is_not_content_and_empty_delete_removes_it() {
    let dir = temp_dir("delete-unsubmitted-link");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    pool.close().await;

    let store = open(&dir).await;
    let reference = store
        .put(b"linked but not committed", "text/plain", at())
        .await
        .expect("put attachment");
    store
        .link(
            &session(),
            &reference.id,
            AttachmentGeneration::new(1),
            at(),
        )
        .await
        .expect("link attachment");
    assert!(
        !store.has_content(&session()).await.expect("classify link"),
        "an attachment link without a saved message or media reference is not content"
    );

    assert!(matches!(
        store
            .delete_session(deletion_write(REQUEST))
            .await
            .expect("delete empty session with unsubmitted link"),
        SessionDeletionOutcome::Deleted { .. }
    ));
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment_link").await,
        0
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        1,
        "the content-addressed attachment row remains for other owners or later GC"
    );
    pool.close().await;
}

#[tokio::test]
async fn committed_message_with_media_reference_blocks_empty_delete() {
    let dir = temp_dir("delete-committed-media");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    pool.close().await;

    let store = open(&dir).await;
    let reference = store
        .put(b"committed media", "image/png", at())
        .await
        .expect("put attachment");
    store
        .link(
            &session(),
            &reference.id,
            AttachmentGeneration::new(1),
            at(),
        )
        .await
        .expect("link attachment to the formal session");
    let content = format!(
        r#"[{{"type":"image_ref","attachmentId":"{}","mimeType":"image/png","byteLength":"15","displayState":"available"}}]"#,
        reference.id.as_str()
    );
    store
        .commit(OwnedCommit {
            session: Some(session()),
            at: at(),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![PendingEvent::new(
                EventKind::FinalMessage,
                EventType::new("agent.message.completed").expect("event type"),
                StoredPolicy::Durable,
                EventPayload::new(
                    ViewJson::new(&format!(
                        r#"{{"messageId":"{MESSAGE}","content":{content}}}"#
                    ))
                    .expect("message view"),
                    None,
                ),
                EventOrigin::Agent,
                None,
                None,
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
            input: None,
        })
        .await
        .expect("commit message with media through the product path");
    assert!(
        store.has_content(&session()).await.expect("classify media"),
        "a saved message with a media reference is content"
    );
    assert!(matches!(
        store
            .delete_session(deletion_write(REQUEST))
            .await
            .expect("not-empty is a failed terminal outcome"),
        SessionDeletionOutcome::Failed {
            blocker: SessionDeletionBlocker::NotEmpty,
            ..
        }
    ));
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_message").await,
        1,
        "a failed delete must preserve the formal message"
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_media_ref").await,
        1,
        "a failed delete must preserve the message's media reference"
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment_link").await,
        1,
        "a failed delete must preserve the formal session's attachment link"
    );
    pool.close().await;
}

#[tokio::test]
async fn deletion_success_is_ordered_replayable_and_keeps_minimal_tombstone() {
    let dir = temp_dir("delete-success");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_attachment_header(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    pool.close().await;

    let store = open(&dir).await;
    assert!(
        !store.has_content(&session()).await.expect("empty session"),
        "the seeded session must be empty"
    );
    let outcome = store
        .delete_session(deletion_write(REQUEST))
        .await
        .expect("delete empty session");
    let (_deletion_event, terminal_event) = match &outcome {
        SessionDeletionOutcome::Deleted {
            deletion_event,
            terminal_event,
        } => (deletion_event.clone(), terminal_event.clone()),
        other => panic!("unexpected deletion outcome: {other:?}"),
    };

    let record = store
        .find_request(&request(REQUEST), &Actor::LocalCli)
        .await
        .expect("find deletion command")
        .expect("deletion command retained");
    assert_eq!(record.status(), CommandStatus::Completed);
    assert_eq!(record.terminal_event(), Some(&terminal_event.id));

    let replay = store
        .delete_session(deletion_write(REQUEST))
        .await
        .expect("same request replays");
    assert!(matches!(
        replay,
        SessionDeletionOutcome::Replayed(ref record)
            if record.status() == CommandStatus::Completed
    ));
    store.close().await;

    let pool = raw_pool(&path).await;
    let events: Vec<(String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT event_type, global_sequence, session_sequence
         FROM owned_event ORDER BY global_sequence",
    )
    .fetch_all(&pool)
    .await
    .expect("events");
    assert_eq!(
        events
            .iter()
            .map(|(event_type, _, _)| event_type.as_str())
            .collect::<Vec<_>>(),
        vec!["session.deleted", "command.completed"]
    );
    assert!(events[0].1 < events[1].1);
    assert_eq!(events[0].2, Some(1));
    assert_eq!(events[1].2, Some(2));
    let terminal: (String, String) =
        sqlx::query_as("SELECT status, terminal_event_id FROM owned_command WHERE request_id=?1")
            .bind(REQUEST)
            .fetch_one(&pool)
            .await
            .expect("terminal row");
    assert_eq!(
        terminal,
        ("completed".to_owned(), terminal_event.id.to_string())
    );
    type TombstoneColumns = (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        i64,
        String,
    );
    let tombstone: TombstoneColumns = sqlx::query_as(
        "SELECT title, custom_title, agent_session_id, workspace_cwd,
                workspace_directory_id, current_mode_id, state, version, deleted_at
         FROM owned_session WHERE session_id=?1",
    )
    .bind(SESSION)
    .fetch_one(&pool)
    .await
    .expect("tombstone");
    assert_eq!(
        tombstone,
        (
            None,
            None,
            None,
            None,
            None,
            None,
            "idle".to_owned(),
            2,
            AT.to_owned()
        )
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment").await,
        1
    );
    pool.close().await;
}

#[tokio::test]
async fn session_delete_removes_transfer_part_and_its_durable_locator() {
    let dir = temp_dir("delete-transfer-file");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    insert_transfer(&pool, TRANSFER).await;
    pool.close().await;

    let transfer_path = dir
        .join("attachments")
        .join(".transfers")
        .join(format!("{TRANSFER}.part"));
    std::fs::create_dir_all(transfer_path.parent().expect("transfer parent"))
        .expect("transfer dir");
    std::fs::write(&transfer_path, b"sensitive partial upload").expect("transfer file");

    let store = open(&dir).await;
    assert!(matches!(
        store
            .delete_session(deletion_write(REQUEST))
            .await
            .expect("delete empty session with transfer"),
        SessionDeletionOutcome::Deleted { .. }
    ));
    assert!(
        !transfer_path.exists(),
        "the partial upload file must be removed after the tombstone commits"
    );
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_attachment_transfer WHERE session_id='11111111-1111-4111-8111-111111111111'",
        )
        .await,
        0,
        "the locator row is removed only after the file is gone"
    );
    pool.close().await;
}

#[tokio::test]
async fn transfer_cleanup_failure_never_changes_committed_terminal_and_keeps_locator() {
    let dir = temp_dir("delete-transfer-cleanup-failure");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    insert_transfer(&pool, TRANSFER).await;
    insert_transfer(&pool, OTHER_TRANSFER).await;
    pool.close().await;

    let transfer_path = dir
        .join("attachments")
        .join(".transfers")
        .join(format!("{TRANSFER}.part"));
    let other_transfer_path = dir
        .join("attachments")
        .join(".transfers")
        .join(format!("{OTHER_TRANSFER}.part"));
    std::fs::create_dir_all(&transfer_path).expect("blocking transfer directory");
    std::fs::create_dir_all(&other_transfer_path).expect("second blocking transfer directory");

    let store = open(&dir).await;
    assert!(matches!(
        store
            .delete_session(deletion_write(REQUEST))
            .await
            .expect("post-commit cleanup failure does not turn deletion into an error"),
        SessionDeletionOutcome::Deleted { .. }
    ));
    assert!(matches!(
        store
            .delete_session(deletion_write(REQUEST))
            .await
            .expect("same request still replays completed while cleanup remains blocked"),
        SessionDeletionOutcome::Replayed(ref record)
            if record.status() == CommandStatus::Completed
    ));
    store.close().await;

    let pool = raw_pool(&path).await;
    assert!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT deleted_at FROM owned_session WHERE session_id=?1",
        )
        .bind(SESSION)
        .fetch_one(&pool)
        .await
        .expect("tombstone")
        .is_some(),
        "the logical deletion stays committed when post-commit cleanup fails"
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment_transfer").await,
        2,
        "every failed transfer cleanup must retain its durable locator"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM owned_command WHERE request_id=?1")
        .bind(REQUEST)
        .fetch_one(&pool)
        .await
        .expect("completed delete command");
    assert_eq!(status, "completed");
    pool.close().await;

    std::fs::remove_dir_all(&transfer_path).expect("remove blocking directory");
    std::fs::remove_dir_all(&other_transfer_path).expect("remove second blocking directory");
    std::fs::write(&transfer_path, b"sensitive partial upload").expect("retry transfer file");
    std::fs::write(&other_transfer_path, b"sensitive partial upload")
        .expect("retry second transfer file");
    let store = open(&dir).await;
    assert!(
        !transfer_path.exists(),
        "the next open resumes cleanup from the retained locator"
    );
    assert!(
        !other_transfer_path.exists(),
        "the next open cleans every retained locator"
    );
    assert!(matches!(
        store
            .delete_session(deletion_write(REQUEST))
            .await
            .expect("same deletion request replays after recovery"),
        SessionDeletionOutcome::Replayed(_)
    ));
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment_transfer").await,
        0
    );
    pool.close().await;
}

#[tokio::test]
async fn terminal_only_retries_cannot_replay_through_a_tombstone() {
    let dir = temp_dir("delete-terminal-replay");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    let terminal_cases = [
        (
            "15151515-1515-4515-8515-151515151515",
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "completed",
            "command.completed",
        ),
        (
            "26262626-2626-4626-8626-262626262626",
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            "failed",
            "command.failed",
        ),
        (
            "37373737-3737-4737-8737-373737373737",
            "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            "uncertain",
            "command.uncertain",
        ),
    ];
    for (sequence, (request_id, event_id, status, event_type)) in
        terminal_cases.into_iter().enumerate()
    {
        insert_event(
            &pool,
            event_type,
            event_id,
            i64::try_from(sequence + 1).expect("sequence"),
        )
        .await;
        sqlx::query(
            "INSERT INTO owned_command
               (actor_kind, actor_id, request_id, session_id, command, kind,
                expected_version, request_fingerprint, accepted_at, status,
                terminal_at, terminal_event_id, error_code, error_message, retryable)
             VALUES ('cli', 'cli', ?1, ?2, 'session.title.set', 'mutation',
                     NULL, ?3, ?4, ?5, ?4, ?6, ?7, ?8, 0)",
        )
        .bind(request_id)
        .bind(SESSION)
        .bind(digest_text("terminal replay"))
        .bind(AT)
        .bind(status)
        .bind(event_id)
        .bind((status != "completed").then_some("test_error"))
        .bind((status != "completed").then_some("terminal replay test"))
        .execute(&pool)
        .await
        .expect("terminal command");
    }
    pool.close().await;

    let store = open(&dir).await;
    assert!(matches!(
        store
            .delete_session(deletion_write(REQUEST))
            .await
            .expect("delete empty session"),
        SessionDeletionOutcome::Deleted { .. }
    ));
    store.close().await;
    let pool = raw_pool(&path).await;
    let events_before = table_snapshot(&pool, "owned_event").await;
    let commands_before = table_snapshot(&pool, "owned_command").await;
    pool.close().await;

    let store = open(&dir).await;
    for (request_id, _event_id, status, event_type) in terminal_cases {
        let terminal = CommandTerminalRecord::try_new(
            match status {
                "completed" => CommandStatus::Completed,
                "failed" => CommandStatus::Failed,
                "uncertain" => CommandStatus::Uncertain,
                other => panic!("unexpected status {other}"),
            },
            Some(at()),
            None,
            None,
            (status != "completed").then(|| {
                PublicError::coded("test.terminal", "terminal replay test", false)
                    .expect("public error")
            }),
        )
        .expect("terminal record");
        let error = store
            .commit(OwnedCommit {
                session: Some(session()),
                at: at(),
                expected_version: None,
                state: None,
                turns: Vec::new(),
                events: vec![PendingEvent::new(
                    EventKind::Structured,
                    EventType::new(event_type).expect("event type"),
                    StoredPolicy::Durable,
                    EventPayload::new(
                        ViewJson::new(r#"{"requestId":"terminal retry"}"#)
                            .expect("terminal replay view"),
                        None,
                    ),
                    EventOrigin::LocalCli,
                    None,
                    Some(request(request_id)),
                )],
                interactions: Vec::new(),
                compacted: Vec::new(),
                idempotency: None,
                command_terminal: Some(terminal),
                origin_epoch: None,
                input: None,
            })
            .await
            .expect_err("terminal-only retry must not replay through a tombstone");
        assert!(
            matches!(error, PortError::NotFound(acp_core::model::EntityRef::Session(ref id)) if id == &session()),
            "{status} terminal replay returned the wrong error: {error:?}"
        );
    }
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(table_snapshot(&pool, "owned_event").await, events_before);
    assert_eq!(
        table_snapshot(&pool, "owned_command").await,
        commands_before
    );
    pool.close().await;
}

#[tokio::test]
async fn not_empty_final_check_commits_failed_terminal_and_preserves_content() {
    let dir = temp_dir("delete-not-empty");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    seed_content_family(&pool, "owned_message").await;
    insert_delete_command(&pool, REQUEST).await;
    pool.close().await;

    let store = open(&dir).await;
    assert_eq!(
        store
            .preflight(&deletion_write(REQUEST))
            .await
            .expect("read-only deletion preflight"),
        SessionDeletionEligibility::Blocked(SessionDeletionBlocker::NotEmpty),
        "content must remain a distinct preflight reason"
    );
    let outcome = store
        .delete_session(deletion_write(REQUEST))
        .await
        .expect("not-empty is a committed failed outcome");
    assert!(matches!(
        outcome,
        SessionDeletionOutcome::Failed {
            blocker: SessionDeletionBlocker::NotEmpty,
            ..
        }
    ));
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_message").await,
        1
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_event WHERE event_type='session.deleted'",
        )
        .await,
        0
    );
}

#[tokio::test]
async fn protected_final_check_commits_failed_terminal_without_cleanup() {
    let dir = temp_dir("delete-protected");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    insert_other_mutation(&pool, OTHER_REQUEST).await;
    pool.close().await;

    let store = open(&dir).await;
    assert_eq!(
        store
            .preflight(&deletion_write(REQUEST))
            .await
            .expect("read-only deletion preflight"),
        SessionDeletionEligibility::Blocked(SessionDeletionBlocker::Protected),
        "the preflight must expose another accepted mutation without writing a terminal"
    );
    let outcome = store
        .delete_session(deletion_write(REQUEST))
        .await
        .expect("protected is a committed failed outcome");
    assert!(matches!(
        outcome,
        SessionDeletionOutcome::Failed {
            blocker: SessionDeletionBlocker::Protected,
            ..
        }
    ));
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_event WHERE event_type='command.failed'",
        )
        .await,
        1
    );
    let status: String = sqlx::query_scalar("SELECT status FROM owned_command WHERE request_id=?1")
        .bind(REQUEST)
        .fetch_one(&pool)
        .await
        .expect("deletion status");
    assert_eq!(status, "failed");
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_session WHERE deleted_at IS NULL"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn deleted_target_is_not_found_and_retry_is_still_terminal() {
    let dir = temp_dir("delete-not-found");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    pool.close().await;

    let store = open(&dir).await;
    assert!(matches!(
        store
            .delete_session(deletion_write(REQUEST))
            .await
            .expect("first delete"),
        SessionDeletionOutcome::Deleted { .. }
    ));
    store.close().await;

    let pool = raw_write_pool(&path).await;
    insert_delete_command(&pool, OTHER_REQUEST).await;
    pool.close().await;
    let store = open(&dir).await;
    assert!(matches!(
        store
            .delete_session(deletion_write(OTHER_REQUEST))
            .await
            .expect("second delete"),
        SessionDeletionOutcome::Failed {
            blocker: SessionDeletionBlocker::NotFound,
            ..
        }
    ));
    store.close().await;

    let pool = raw_pool(&path).await;
    let status: String = sqlx::query_scalar("SELECT status FROM owned_command WHERE request_id=?1")
        .bind(OTHER_REQUEST)
        .fetch_one(&pool)
        .await
        .expect("not-found status");
    assert_eq!(status, "failed");
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_event WHERE event_type='session.deleted'",
        )
        .await,
        1
    );
}

#[tokio::test]
async fn transactional_failure_rolls_back_rows_versions_and_sequences_before_retry() {
    let dir = temp_dir("delete-rollback");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    insert_transfer(&pool, TRANSFER).await;
    sqlx::raw_sql(
        "CREATE TRIGGER fail_tombstone BEFORE UPDATE ON owned_session
         WHEN NEW.deleted_at IS NOT NULL
         BEGIN
           SELECT RAISE(ABORT, 'injected delete failure');
         END;",
    )
    .execute(&pool)
    .await
    .expect("failure trigger");
    let before = (
        table_snapshot(&pool, "owned_session").await,
        table_snapshot(&pool, "owned_event").await,
        table_snapshot(&pool, "owned_command").await,
        table_snapshot(&pool, "owned_attachment_transfer").await,
        table_snapshot(&pool, "sqlite_sequence").await,
        meta_rows(&pool).await,
    );
    pool.close().await;
    let transfer_path = dir
        .join("attachments")
        .join(".transfers")
        .join(format!("{TRANSFER}.part"));
    std::fs::create_dir_all(transfer_path.parent().expect("transfer parent"))
        .expect("transfer directory");
    std::fs::write(&transfer_path, b"must survive SQL rollback").expect("transfer file");

    let store = open(&dir).await;
    let _error = store
        .delete_session(deletion_write(REQUEST))
        .await
        .expect_err("injected SQL failure must abort the transaction");
    store.close().await;

    let pool = raw_write_pool(&path).await;
    assert_eq!(
        (
            table_snapshot(&pool, "owned_session").await,
            table_snapshot(&pool, "owned_event").await,
            table_snapshot(&pool, "owned_command").await,
            table_snapshot(&pool, "owned_attachment_transfer").await,
            table_snapshot(&pool, "sqlite_sequence").await,
            meta_rows(&pool).await,
        ),
        before,
        "SQL failure must roll back every row, version, sequence, and transfer locator"
    );
    assert!(
        transfer_path.exists(),
        "filesystem cleanup runs only after the deletion transaction commits"
    );
    sqlx::raw_sql("DROP TRIGGER fail_tombstone;")
        .execute(&pool)
        .await
        .expect("drop trigger");
    pool.close().await;

    let store = open(&dir).await;
    let retried = store
        .delete_session(deletion_write(REQUEST))
        .await
        .expect("retry after rollback");
    assert!(matches!(retried, SessionDeletionOutcome::Deleted { .. }));
    store.close().await;
    assert!(!transfer_path.exists());
    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT MIN(global_sequence) FROM owned_event",).await,
        1,
        "the rolled-back ids must not be burned"
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_attachment_transfer").await,
        0,
        "the successful retry removes the transfer locator after deleting its file"
    );
}

#[tokio::test]
async fn tombstone_is_hidden_from_every_product_read_but_global_replay_stays_visible() {
    let dir = temp_dir("delete-visibility");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    pool.close().await;

    let store = open(&dir).await;
    let outcome = store
        .delete_session(deletion_write(REQUEST))
        .await
        .expect("delete");
    let deletion_event = match outcome {
        SessionDeletionOutcome::Deleted { deletion_event, .. } => deletion_event,
        other => panic!("unexpected outcome: {other:?}"),
    };
    store.close().await;

    let pool = raw_write_pool(&path).await;
    insert_other_mutation(&pool, OTHER_REQUEST).await;
    seed_tombstoned_direct_resources(&pool).await;
    sqlx::query(
        "INSERT INTO owned_turn
           (turn_id, session_id, state, queue_index, causation, started_at, ended_at)
         VALUES (?1, ?2, 'queued', 0, NULL, NULL, NULL)",
    )
    .bind(TURN)
    .bind(SESSION)
    .execute(&pool)
    .await
    .expect("latent turn behind tombstone");
    pool.close().await;

    let store = open(&dir).await;
    assert!(store.load(&session()).await.expect("load").is_none());
    assert!(
        store
            .list(SessionQuery {
                only: Some(vec![session()]),
                states: Vec::new(),
                limit: None,
            })
            .await
            .expect("list")
            .is_empty()
    );
    assert!(
        store
            .origin_epochs(&[session()])
            .await
            .expect("origins")
            .is_empty()
    );
    assert!(
        store
            .load_recovery(&session())
            .await
            .expect("recovery")
            .is_none()
    );
    assert!(matches!(
        store.session_activity(&session()).await,
        Err(PortError::NotFound(_))
    ));
    assert!(
        store
            .queued_turns(&session())
            .await
            .expect("queued turns")
            .is_empty()
    );
    assert!(
        store
            .sessions_with_pending_activity(ReplayLimit::new(10))
            .await
            .expect("pending activity")
            .is_empty()
    );
    assert!(
        store
            .unsettled_commands(ReplayLimit::new(10))
            .await
            .expect("unsettled")
            .iter()
            .all(|record| record.session() != Some(&session())),
        "accepted commands behind a tombstone are not recovery candidates"
    );
    assert!(
        store
            .retention_window(&session())
            .await
            .expect("retention")
            .is_none()
    );
    assert!(
        store
            .find_request(&request(OTHER_REQUEST), &Actor::LocalCli)
            .await
            .expect("other request lookup")
            .is_none()
    );
    assert_eq!(
        store
            .find_request(&request(REQUEST), &Actor::LocalCli)
            .await
            .expect("delete replay lookup")
            .expect("delete command remains replayable")
            .status(),
        CommandStatus::Completed
    );

    let ordinary_replay = store
        .commit(OwnedCommit {
            session: Some(session()),
            at: at(),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: Some(IdempotencyRecord {
                actor: Actor::LocalCli,
                request: request(OTHER_REQUEST),
                command: "session.title.set".to_owned(),
                kind: CommandKind::Mutation,
                session: Some(session()),
                expected_version: None,
                request_fingerprint: Digest::new(&digest_text("other mutation"))
                    .expect("request fingerprint"),
                accepted_at: at(),
            }),
            command_terminal: None,
            origin_epoch: None,
            input: None,
        })
        .await;
    assert!(
        matches!(ordinary_replay, Err(PortError::NotFound(_))),
        "an ordinary idempotency replay must not expose a request behind a tombstone"
    );

    let transfer = AttachmentId::new(TRANSFER).expect("transfer id");
    assert!(
        store
            .transfer(&transfer)
            .await
            .expect("transfer read")
            .is_none(),
        "a latent transfer behind a tombstone is not visible"
    );
    let submission = SubmissionId::new(SUBMISSION).expect("submission id");
    assert!(
        store
            .submission(&submission)
            .await
            .expect("submission read")
            .is_none(),
        "a latent submission behind a tombstone is not visible"
    );
    let attachment = AttachmentId::new(ATTACHMENT).expect("attachment id");
    assert!(matches!(
        store
            .read_chunk(&AttachmentRead {
                attachment,
                session: session(),
                offset: 0,
                max_length: 32,
            })
            .await,
        Err(PortError::NotFound(_))
    ));
    assert!(matches!(
        store
            .delete_session(deletion_write(REQUEST))
            .await
            .expect("the deletion command itself remains replayable"),
        SessionDeletionOutcome::Replayed(_)
    ));

    let view = store.read_view().await.expect("view");
    let query = HistoryQuery {
        origin_order: false,
        session: session(),
        include: HistoryInclude {
            messages: true,
            turns: true,
            pending_interactions: true,
            config_options: true,
            capabilities: true,
            history: true,
        },
        after: None,
        before_history: None,
        before_messages: None,
        messages_limit: ReplayLimit::new(10),
        limit: ReplayLimit::new(10),
    };
    assert!(matches!(
        view.read_session(query.clone()).await,
        Err(PortError::NotFound(_))
    ));
    assert!(matches!(
        view.read_session_origin(query).await,
        Err(PortError::NotFound(_))
    ));
    assert!(matches!(
        view.node_link_slice(&session(), None, ReplayLimit::new(10))
            .await,
        Err(PortError::NotFound(_))
    ));
    assert!(
        view.session_event_payload(&session(), &deletion_event.id)
            .await
            .expect("session payload")
            .is_none()
    );
    assert!(
        view.event_payload(&deletion_event.id)
            .await
            .expect("global payload")
            .is_some()
    );

    let replay = view
        .replay(None, ReplayLimit::new(10))
        .await
        .expect("global replay");
    assert_eq!(
        replay
            .events
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>(),
        vec!["session.deleted", "command.completed"]
    );
    let sync_replay = view
        .sync_replay(None, ReplayLimit::new(10))
        .await
        .expect("sync replay");
    assert_eq!(
        sync_replay
            .events
            .iter()
            .map(|event| event.event.event_type.as_str())
            .collect::<Vec<_>>(),
        vec!["session.deleted", "command.completed"]
    );
    drop(view);
    store.close().await;
}

#[tokio::test]
async fn deletion_idempotency_conflicts_on_fingerprint_and_session() {
    let dir = temp_dir("delete-idempotency-conflict");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    bootstrap_session_id(&pool, OTHER_SESSION, OTHER_EPOCH).await;
    insert_delete_command(&pool, REQUEST).await;
    pool.close().await;

    let store = open(&dir).await;
    let changed_fingerprint = SessionDeletionWrite::new(
        session(),
        Actor::LocalCli,
        request(REQUEST),
        Digest::new(&digest_text("different delete payload")).expect("request fingerprint"),
        at(),
    );
    assert!(matches!(
        store.delete_session(changed_fingerprint).await,
        Err(PortError::Conflict(ConflictKind::IdempotencyConflict))
    ));

    let changed_session = SessionDeletionWrite::new(
        SessionId::new(OTHER_SESSION).expect("other session"),
        Actor::LocalCli,
        request(REQUEST),
        request_fingerprint(),
        at(),
    );
    assert!(matches!(
        store.delete_session(changed_session).await,
        Err(PortError::Conflict(ConflictKind::IdempotencyConflict))
    ));
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_session WHERE deleted_at IS NULL"
        )
        .await,
        2,
        "an idempotency conflict must not delete either session"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM owned_command WHERE request_id=?1")
        .bind(REQUEST)
        .fetch_one(&pool)
        .await
        .expect("command status");
    assert_eq!(status, "accepted");
}

#[tokio::test]
async fn full_actor_request_timestamp_and_session_identity_are_used() {
    let dir = temp_dir("delete-identity");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    pool.close().await;

    let write = deletion_write(REQUEST);
    assert_eq!(write.session().as_str(), SESSION);
    assert_eq!(write.actor(), &Actor::LocalCli);
    assert_eq!(write.request().as_str(), REQUEST);
    assert_eq!(
        write.request_fingerprint().as_str(),
        request_fingerprint().as_str()
    );
    assert_eq!(write.at().as_str(), AT);

    let store = open(&dir).await;
    store
        .delete_session(write)
        .await
        .expect("full identity write is accepted");
    store.close().await;
}

#[tokio::test]
async fn content_arriving_after_delete_admission_terminates_the_old_request_without_deletion() {
    let dir = temp_dir("delete-consumed-generation");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    pool.close().await;

    let store = open(&dir).await;
    let reference = store
        .put(b"retained after activation", "text/plain", at())
        .await
        .expect("bounded attachment evidence");
    store
        .link(
            &session(),
            &reference.id,
            AttachmentGeneration::new(1),
            at(),
        )
        .await
        .expect("link the activation-side content marker");
    let content = format!(
        r#"[{{"type":"image_ref","attachmentId":"{}","mimeType":"text/plain","byteLength":"5","displayState":"available"}}]"#,
        reference.id.as_str()
    );
    let message = "55555555-5555-4555-8555-555555555555";
    store
        .commit(OwnedCommit {
            session: Some(session()),
            at: at(),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![PendingEvent::new(
                EventKind::FinalMessage,
                EventType::new("agent.message.completed").expect("event type"),
                StoredPolicy::Durable,
                EventPayload::new(
                    ViewJson::new(&format!(
                        r#"{{"messageId":"{message}","content":{content}}}"#
                    ))
                    .expect("message payload"),
                    None,
                ),
                EventOrigin::Agent,
                None,
                None,
            )],
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
            input: None,
        })
        .await
        .expect("commit the new generation content");

    let outcome = store
        .delete_session(generation_write(REQUEST, 1))
        .await
        .expect("the old accepted request gets a failed terminal");
    let event = match outcome {
        SessionDeletionOutcome::Failed {
            blocker: SessionDeletionBlocker::NotEmpty,
            terminal_event,
        } => terminal_event,
        other => panic!("unexpected late-content deletion outcome: {other:?}"),
    };
    assert_eq!(event.event_type.as_str(), "command.failed");
    assert!(
        store
            .has_content(&session())
            .await
            .expect("retained content")
    );
    store.close().await;

    let pool = raw_pool(&path).await;
    let status: String = sqlx::query_scalar("SELECT status FROM owned_command WHERE request_id=?1")
        .bind(REQUEST)
        .fetch_one(&pool)
        .await
        .expect("command status");
    assert_eq!(status, "failed");
    let committed_terminal_event: Option<String> =
        sqlx::query_scalar("SELECT terminal_event_id FROM owned_command WHERE request_id=?1")
            .bind(REQUEST)
            .fetch_one(&pool)
            .await
            .expect("terminal event id");
    assert_eq!(
        committed_terminal_event.as_deref(),
        Some(event.id.as_str()),
        "the terminal must reference the event committed in the same transaction"
    );
    let failed_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM owned_event WHERE event_id=?1 AND event_type='command.failed'",
    )
    .bind(event.id.as_str())
    .fetch_one(&pool)
    .await
    .expect("linked failure event");
    assert_eq!(failed_events, 1);
    let tombstones: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM owned_event WHERE session_id=?1 AND event_type='session.deleted'",
    )
    .bind(SESSION)
    .fetch_one(&pool)
    .await
    .expect("tombstone count");
    assert_eq!(
        tombstones, 0,
        "the old request must not publish a tombstone"
    );
    let messages: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM owned_message WHERE session_id=?1")
            .bind(SESSION)
            .fetch_one(&pool)
            .await
            .expect("message count");
    assert_eq!(
        messages, 1,
        "activated content must survive the old deletion"
    );
    pool.close().await;
}

#[tokio::test]
async fn a_not_empty_before_release_gets_a_linked_failure_event() {
    let dir = temp_dir("delete-early-not-empty");
    let store = open(&dir).await;
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    bootstrap_session(&pool).await;
    insert_delete_command(&pool, REQUEST).await;
    seed_content_family(&pool, "owned_user_input").await;
    pool.close().await;

    let store = open(&dir).await;
    let outcome = store
        .delete_session(deletion_write(REQUEST))
        .await
        .expect("the content blocker is a persisted terminal");
    assert!(matches!(
        outcome,
        SessionDeletionOutcome::Failed {
            blocker: SessionDeletionBlocker::NotEmpty,
            ..
        }
    ));
    store.close().await;

    let pool = raw_pool(&path).await;
    let terminal_event: Option<String> =
        sqlx::query_scalar("SELECT terminal_event_id FROM owned_command WHERE request_id=?1")
            .bind(REQUEST)
            .fetch_one(&pool)
            .await
            .expect("terminal event id");
    assert!(terminal_event.is_some());
    let failed_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM owned_event WHERE event_id=?1 AND event_type='command.failed'",
    )
    .bind(terminal_event.as_deref().expect("terminal event"))
    .fetch_one(&pool)
    .await
    .expect("linked failure event");
    assert_eq!(failed_events, 1);
    let turns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM owned_turn WHERE session_id=?1")
        .bind(SESSION)
        .fetch_one(&pool)
        .await
        .expect("turn count");
    assert_eq!(
        turns, 1,
        "the failure transaction must not remove accepted content"
    );
    pool.close().await;
}

#[tokio::test]
async fn fresh_schema_and_migration_metadata_use_v19_and_drop_preparation_table() {
    let dir = temp_dir("delete-schema-v19");
    let store = open(&dir).await;
    store.close().await;
    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(scalar_i64(&pool, "PRAGMA user_version").await, 19);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT value FROM meta WHERE key='owned_schema_version'",)
            .fetch_one(&pool)
            .await
            .expect("owned schema version"),
        "19"
    );
    assert_eq!(
        column_names(&pool, "owned_session")
            .await
            .last()
            .map(String::as_str),
        Some("deleted_at")
    );
    assert!(!table_exists(&pool, "owned_first_submission").await);
    pool.close().await;
}
