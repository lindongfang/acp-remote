//! §7.3/§7.4 的每个枚举列（`IN (...)` 或等值 CHECK）必须与 `core` 的枚举表**逐值一致**，且取值都要能原样落库。
//!
//! 这些判据的取值集来自 `core` 的 `ALL` + `as_str()`，不手抄字面量：漏一个变体（例如
//! `ElicitationAction::Decline` 曾只写在 `('submit','cancel')` 里）会让 Agent 侧的 `decline` 撞约束、
//! 事务回滚、交互永远停在 `pending`。

mod support;

use acp_core::model::{
    ActorKind, AuditAction, AuditOutcome, CachePolicy, CommandKind, CommandStatus, DeviceState,
    ElicitationAction, ElicitationValues, EventKind, EventOrigin, InteractionId, InteractionKind,
    InteractionResolution, NodeKind, NodeState, PairingState, PairingTarget,
    PermissionDecisionKind, ProviderRefKind, RawUnavailableReason, SessionId, SessionState,
    StoredPolicy, Timestamp, TurnState,
};
use acp_core::ports::{InteractionResolved, ModeChange, SessionStore, SessionUpdate, StateChange};
use support::*;

/// 从 `sqlite_master.sql` 里取 CHECK 约束的允许取值集合。
///
/// 支持两种 DDL 形状：`<column> IN ('a','b',...)` 与等值约束 `<column> = 'a'`（`cache_policy` 的单值 CHECK）。
/// 用词边界匹配列名：`kind IN (` 会命中 `actor_kind IN (` 的尾部（第一次就是这样误报的）。
fn enum_values(sql: &str, column: &str) -> Vec<String> {
    if let Some(values) = in_values(sql, column) {
        return values;
    }
    vec![eq_value(sql, column)]
}

/// `<column> IN ('a','b',...)` 的取值集合；形状不匹配时返回 `None`。
fn in_values(sql: &str, column: &str) -> Option<Vec<String>> {
    let pattern = format!(r"\b{column}\s+IN\s*\(");
    let regex = regex_lite(&pattern);
    let (_, end_of_match) = regex.find(sql)?;
    let rest = &sql[end_of_match..];
    let end = rest.find(')')?;
    Some(
        rest[..end]
            .split(',')
            .map(|item| item.trim().trim_matches('\'').to_owned())
            .collect(),
    )
}

/// 等值 CHECK `<column> = 'a'` 的唯一取值；两种形状都不匹配时 panic（列名写错或 DDL 形状变了）。
fn eq_value(sql: &str, column: &str) -> String {
    let pattern = format!(r"\b{column}\s+=");
    let regex = regex_lite(&pattern);
    let (_, value_start) = regex
        .find_assignment(sql)
        .unwrap_or_else(|| panic!("no `{column} IN (...)` or `{column} = '...'` in:\n{sql}"));
    let rest = &sql[value_start..];
    let end = rest.find('\'').expect("closing quote");
    rest[..end].to_owned()
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
            .split("\\s+")
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

    /// 返回 `(匹配起点, 开头单引号之后的位置)`，仅匹配 `<ident> = '`（DDL 的等值 CHECK）。
    fn find_assignment(&self, haystack: &str) -> Option<(usize, usize)> {
        let needle = &self.ident;
        let bytes = haystack.as_bytes();
        let mut from = 0;
        while let Some(offset) = haystack[from..].find(needle) {
            let start = from + offset;
            let before_ok =
                start == 0 || !bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_';
            let after = start + needle.len();
            let tail = &haystack[after..];
            let trimmed = tail.trim_start();
            if before_ok && trimmed.starts_with('=') {
                let quote = trimmed.find('\'')?;
                let after_ident = after + (tail.len() - trimmed.len());
                return Some((start, after_ident + quote + 1));
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

/// §7.3 的 `revoke_reason` CHECK 允许值（`core::ports::RevokeReason` 的落库 token）。
///
/// 期望值刻意锚在合同文本而不是 `storage_sqlite` 的 `revoke_token()`：两者来源相互独立，
/// 「DDL 与映射同时拼错」才会同时被本断言与行为回归发现。core 侧没有 `ALL`/`as_str`，
/// 也不为测试新增这类公开 API。
fn revoke_reason_tokens() -> Vec<String> {
    ["user_requested", "key_changed", "compromised"]
        .into_iter()
        .map(str::to_owned)
        .collect()
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
    let cases: [(&str, &str, Vec<String>); 30] = [
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
        (
            "owned_audit",
            "actor_kind",
            tokens(ActorKind::ALL, ActorKind::as_str),
        ),
        (
            "imported_audit",
            "actor_kind",
            tokens(ActorKind::ALL, ActorKind::as_str),
        ),
        // v2 管理表的枚举列（§11.7 的九张表 + 两个索引）同样必须与 core 逐值一致。`peer_kind` 的
        // 取值集与 `PairingTarget` 同源（`device`/`node`），用它的 `ALL` 断言以免手抄字面量。
        (
            "owned_device",
            "state",
            tokens(DeviceState::ALL, DeviceState::as_str),
        ),
        (
            "owned_node",
            "kind",
            tokens(NodeKind::ALL, NodeKind::as_str),
        ),
        (
            "owned_node",
            "state",
            tokens(NodeState::ALL, NodeState::as_str),
        ),
        (
            "owned_peer_key",
            "peer_kind",
            tokens(PairingTarget::ALL, PairingTarget::as_str),
        ),
        (
            "owned_pairing",
            "target_kind",
            tokens(PairingTarget::ALL, PairingTarget::as_str),
        ),
        (
            "owned_pairing",
            "state",
            tokens(PairingState::ALL, PairingState::as_str),
        ),
        (
            "owned_pairing_peer",
            "peer_kind",
            tokens(PairingTarget::ALL, PairingTarget::as_str),
        ),
        (
            "owned_provider_ref",
            "kind",
            tokens(ProviderRefKind::ALL, ProviderRefKind::as_str),
        ),
        // v2 管理表的 `revoke_reason` 是 `core::ports::RevokeReason` 的落库标记（token 属 storage-sqlite，
        // 见 `src/admin/trust.rs` 的 `revoke_token()`）。core 侧没有 `ALL`/`as_str`，也不为测试新增公开
        // API，因此期望值锚在 §7.3 的合同文本；行为面由 `admin_store.rs` 的 KeyChanged 回归覆盖。
        ("owned_device", "revoke_reason", revoke_reason_tokens()),
        ("owned_node", "revoke_reason", revoke_reason_tokens()),
        // `cache_policy` 是等值 CHECK（单值）；期望值来自 `core::model::CachePolicy`，因此同时断言
        // 「DDL 是单值」与「与 core 当前取值一致」——core 增加第二取值而 DDL 仍单值时本用例即失败。
        (
            "owned_export",
            "cache_policy",
            tokens(CachePolicy::ALL, CachePolicy::as_str),
        ),
        (
            "imported_import",
            "cache_policy",
            tokens(CachePolicy::ALL, CachePolicy::as_str),
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
            compacted: Vec::new(),
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
            compacted: Vec::new(),
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
