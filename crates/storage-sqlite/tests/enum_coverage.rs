//! §7.3/§7.4 的每个 `IN (...)` 枚举列必须与 `core` 的枚举表**逐值一致**，且取值都要能原样落库。
//!
//! 这些判据的取值集来自 `core` 的 `ALL` + `as_str()`，不手抄字面量：漏一个变体（例如
//! `ElicitationAction::Decline` 曾只写在 `('submit','cancel')` 里）会让 Agent 侧的 `decline` 撞约束、
//! 事务回滚、交互永远停在 `pending`。

mod support;

use acp_core::model::{
    ActorKind, AuditAction, AuditOutcome, CommandKind, CommandStatus, ElicitationAction,
    ElicitationValues, EventKind, EventOrigin, InteractionId, InteractionKind,
    InteractionResolution, PermissionDecisionKind, RawUnavailableReason, SessionId, SessionState,
    StoredPolicy, Timestamp, TurnState,
};
use acp_core::ports::{InteractionResolved, ModeChange, SessionStore, SessionUpdate, StateChange};
use support::*;

/// 从 `sqlite_master.sql` 里取 `CHECK (<column> IN ('a','b',...))` 的取值集合。
///
/// 用词边界匹配列名：`kind IN (` 会命中 `actor_kind IN (` 的尾部（第一次就是这样误报的）。
fn enum_values(sql: &str, column: &str) -> Vec<String> {
    let pattern = format!(r"\b{column}\s+IN\s*\(");
    let regex = regex_lite(&pattern);
    let (start, end_of_match) = regex
        .find(sql)
        .unwrap_or_else(|| panic!("no `{column} IN (...)` in:\n{sql}"));
    let rest = &sql[end_of_match..];
    let end = rest.find(')').expect("closing paren");
    let values = rest[..end]
        .split(',')
        .map(|item| item.trim().trim_matches('\'').to_owned())
        .collect();
    let _ = start;
    values
}

/// 极简正则：只支持 `\b<ident>\s+IN\s*\(` 这一种形状（避免为一个测试引入依赖）。
fn regex_lite(pattern: &str) -> ManualPattern {
    ManualPattern::parse(pattern)
}

/// `\b<ident>\s+IN\s*\(` 的专用匹配器。
struct ManualPattern {
    ident: String,
}

impl ManualPattern {
    fn parse(pattern: &str) -> Self {
        let ident = pattern
            .trim_start_matches("\\b")
            .split("\\s+IN")
            .next()
            .expect("ident")
            .to_owned();
        Self { ident }
    }

    /// 返回 `(匹配起点, 左括号之后的位置)`。
    fn find(&self, haystack: &str) -> Option<(usize, usize)> {
        let needle = &self.ident;
        let bytes = haystack.as_bytes();
        let mut from = 0;
        while let Some(offset) = haystack[from..].find(needle) {
            let start = from + offset;
            let before_ok =
                start == 0 || !bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_';
            let after = start + needle.len();
            let tail = &haystack[after..];
            let tail = tail.trim_start();
            if before_ok && tail.starts_with("IN") {
                let paren = tail.find('(')?;
                return Some((
                    start,
                    after + (haystack[after..].len() - tail.len()) + paren + 1,
                ));
            }
            from = after;
            if from >= haystack.len() {
                break;
            }
        }
        None
    }
}

fn tokens<T: Copy>(all: &[T], as_str: impl Fn(T) -> &'static str) -> Vec<String> {
    all.iter().map(|value| as_str(*value).to_owned()).collect()
}

/// 每张表的 DDL 文本，按表名索引。
async fn table_sql(pool: &sqlx::SqlitePool, table: &str) -> String {
    let sql: Option<String> =
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1")
            .bind(table)
            .fetch_one(pool)
            .await
            .expect("table sql");
    sql.unwrap_or_else(|| panic!("{table} is missing"))
}

/// DDL 的枚举取值集必须等于 `core` 枚举的取值集（双向：既不缺也不多）。
#[tokio::test]
async fn ddl_enum_lists_match_the_core_enums() {
    let dir = temp_dir("enum-coverage");
    let store = storage_sqlite::session_store::SqliteStore::open(
        storage_sqlite::migrate::StorageConfig::new(&dir),
        &Timestamp::new(AT).expect("timestamp"),
    )
    .await
    .expect("open");
    store.close().await;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    let cases: [(&str, &str, Vec<String>); 16] = [
        (
            "owned_session",
            "state",
            tokens(SessionState::ALL, SessionState::as_str),
        ),
        (
            "owned_turn",
            "state",
            tokens(TurnState::ALL, TurnState::as_str),
        ),
        (
            "owned_event",
            "kind",
            tokens(EventKind::ALL, EventKind::as_str),
        ),
        (
            "owned_event",
            "policy",
            tokens(StoredPolicy::ALL, StoredPolicy::as_str),
        ),
        (
            "owned_event",
            "origin_kind",
            tokens(EventOrigin::ALL, EventOrigin::as_str),
        ),
        (
            "owned_event",
            "acp_raw_unavailable_reason",
            tokens(RawUnavailableReason::ALL, RawUnavailableReason::as_str),
        ),
        (
            "owned_command",
            "actor_kind",
            tokens(ActorKind::ALL, ActorKind::as_str),
        ),
        (
            "owned_command",
            "kind",
            tokens(CommandKind::ALL, CommandKind::as_str),
        ),
        (
            "owned_command",
            "status",
            tokens(CommandStatus::ALL, CommandStatus::as_str),
        ),
        (
            "owned_interaction",
            "kind",
            tokens(InteractionKind::ALL, InteractionKind::as_str),
        ),
        (
            "owned_interaction",
            "decision_kind",
            tokens(PermissionDecisionKind::ALL, PermissionDecisionKind::as_str),
        ),
        (
            "owned_interaction",
            "elicitation_action",
            tokens(ElicitationAction::ALL, ElicitationAction::as_str),
        ),
        (
            "owned_audit",
            "action",
            tokens(AuditAction::ALL, AuditAction::as_str),
        ),
        (
            "owned_audit",
            "outcome",
            tokens(AuditOutcome::ALL, AuditOutcome::as_str),
        ),
        (
            "imported_command_ref",
            "status",
            tokens(CommandStatus::ALL, CommandStatus::as_str),
        ),
        (
            "imported_audit",
            "action",
            tokens(AuditAction::ALL, AuditAction::as_str),
        ),
    ];
    for (table, column, expected) in cases {
        let sql = table_sql(&pool, table).await;
        let mut actual = enum_values(&sql, column);
        actual.sort();
        let mut expected = expected;
        expected.sort();
        assert_eq!(
            actual, expected,
            "{table}.{column} must accept exactly the core enum's values"
        );
    }
    pool.close().await;
}

/// 行为面：`ElicitationAction::Decline`（旧 DDL 只允许 `submit|cancel`）必须能落库并原样读回。
#[tokio::test]
async fn elicitation_decline_round_trips() {
    const EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
    const INTERACTION: &str = "44444444-4444-4444-8444-444444444444";

    let dir = temp_dir("enum-decline");
    let store = storage_sqlite::session_store::SqliteStore::open(
        storage_sqlite::migrate::StorageConfig::new(&dir),
        &Timestamp::new(AT).expect("timestamp"),
    )
    .await
    .expect("open");
    let session: SessionId = store
        .commit(acp_core::ports::OwnedCommit {
            session: None,
            at: Timestamp::new(AT).expect("timestamp"),
            expected_version: None,
            state: Some(StateChange::Create(acp_core::ports::NewSession {
                title: None,
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
            origin_epoch: Some(acp_core::model::OriginEpoch::new(EPOCH).expect("epoch")),
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id");

    let path = dir.join("acp-remote.sqlite3");
    let pool = raw_write_pool(&path).await;
    sqlx::query(
        "INSERT INTO owned_interaction (interaction_id, session_id, kind, state, created_at) \
         VALUES (?1, ?2, 'elicitation', 'pending', ?3)",
    )
    .bind(INTERACTION)
    .bind(session.as_str())
    .bind(AT)
    .execute(&pool)
    .await
    .expect("seed pending elicitation");
    pool.close().await;

    store
        .commit(acp_core::ports::OwnedCommit {
            session: Some(session.clone()),
            at: Timestamp::new("2026-09-18T01:00:00.000Z").expect("timestamp"),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: Some(InteractionResolved {
                    interaction: InteractionId::new(INTERACTION).expect("interaction id"),
                    resolution: InteractionResolution::elicitation(
                        ElicitationAction::Decline,
                        ElicitationValues::null(),
                    )
                    .expect("decline resolution"),
                    resolved_by: acp_core::model::Actor::LocalCli,
                }),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(acp_core::model::OriginEpoch::new(EPOCH).expect("epoch")),
        })
        .await
        .expect("decline must be storable");

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_interaction WHERE state = 'resolved' \
             AND elicitation_action = 'decline'",
        )
        .await,
        1,
        "the declined interaction must reach a terminal row"
    );
    pool.close().await;
    store.close().await;
}
