//! `review-w3-r1-F1` 的存储侧：会话标题「显式置空」与通用状态变更走**同一套**版本与时间语义。
//!
//! 权威是 `docs/CORE_PORTS_AND_STORAGE.md` §5.2（`StateChange::Update` 的窄写列，`title` 为**两层可选**）
//! 与 §6 第 21 条，以及 `design.md` D7（会话的**权威**更新时间取 Daemon 持久化时间）。
//!
//! 判别力（为什么这几条断言能抓住旧实现）：
//! - 旧实现在 `title == Some(None)` 时**替换**整条 `UPDATE`，换成只写 `title` 的窄语句；因此
//!   ① 版本不递增、② `updated_at` 不前进、③ 同一提交里的 `state`/`current_mode_*`/`workspace_alias`
//!   等列被静默丢弃——三条断言各自都会失败；
//! - 该批次还带一条 `session.mode.changed`（`SYNC_PROTOCOL.md` §10.3 要求注入会话 `version`，
//!   由 core 在提交前按「含 `StateChange` ⇒ 当前版本 + 1」填充）。旧实现的未递增返回值会让 core 的
//!   `commit_owned` 诊断出**版本漂移**并返回 `PortError::Corrupt`，整批事件被丢掉；本用例因此还断言
//!   「不返回 `Corrupt`、事件确实落盘、注入值与返回版本一致」——这三条正是 core 的比对判据。

mod support;

use acp_core::model::{
    AgentRef, EventKind, EventOrigin, EventPayload, EventType, ModeId, ModeRef, OriginEpoch,
    PendingEvent, SessionId, SessionState, StoredPolicy, Timestamp, Version, ViewJson,
};
use acp_core::ports::{
    ModeChange, NewSession, OwnedCommit, SessionStore, SessionUpdate, StateChange,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const ORIGIN_EPOCH: &str = "7b1f4d2a-9c3e-4f5a-8b6d-0e1f2a3b4c5d";
/// 置空路径下必须一并落盘的恢复标识（§3.6 的窄写列）。
const AGENT_SESSION_ID: &str = "acp-title-write";
/// 同上：规范化的会话工作目录。
const WORKSPACE_CWD: &str = "C:\\work\\title-write";

fn at(offset_hours: i64) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{offset_hours:02}:00:00.000Z")).expect("timestamp")
}

fn origin_epoch() -> OriginEpoch {
    OriginEpoch::new(ORIGIN_EPOCH).expect("origin epoch")
}

fn agent() -> AgentRef {
    AgentRef::try_new(
        acp_core::model::AgentId::new("probe-agent").expect("agent id"),
        "Probe Agent",
    )
    .expect("agent ref")
}

/// 一条已按 §10.3 注入 `version` 的 `session.mode.changed`（core 的注入结果就是这种文本）。
fn mode_changed(version: u64) -> PendingEvent {
    PendingEvent::new(
        EventKind::State,
        EventType::new("session.mode.changed").expect("event type"),
        StoredPolicy::Durable,
        EventPayload::new(
            ViewJson::new(&format!(
                r#"{{"version":"{version}","currentModeId":"code"}}"#
            ))
            .expect("view json"),
            None,
        ),
        EventOrigin::Agent,
        None,
        None,
    )
}

fn create_session(title: Option<&str>) -> OwnedCommit {
    OwnedCommit {
        session: None,
        at: at(0),
        expected_version: None,
        state: Some(StateChange::Create(NewSession {
            title: title.map(str::to_owned),
            agent: agent(),
        })),
        turns: Vec::new(),
        events: Vec::new(),
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: None,
        command_terminal: None,
        origin_epoch: Some(origin_epoch()),
    }
}

fn update(
    session: &SessionId,
    at: Timestamp,
    expected_version: Option<Version>,
    state: Option<StateChange>,
    events: Vec<PendingEvent>,
) -> OwnedCommit {
    OwnedCommit {
        session: Some(session.clone()),
        at,
        expected_version,
        state,
        turns: Vec::new(),
        events,
        interactions: Vec::new(),
        compacted: Vec::new(),
        idempotency: None,
        command_terminal: None,
        origin_epoch: None,
    }
}

/// 会话的全部窄写列 + 版本 + 更新时间（逐条断言的读面）。
struct Row {
    title: Option<String>,
    agent_session_id: Option<String>,
    workspace_cwd: Option<String>,
    state: String,
    mode_id: Option<String>,
    mode_name: Option<String>,
    workspace_alias: Option<String>,
    version: i64,
    updated_at: String,
}

async fn row(dir: &std::path::Path, session: &SessionId) -> Row {
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    let pool = raw_pool(&path).await;
    let record = sqlx::query(
        "SELECT title, agent_session_id, workspace_cwd, state, current_mode_id, \
         current_mode_name, workspace_alias, version, updated_at \
         FROM owned_session WHERE session_id = ?1",
    )
    .bind(session.as_str())
    .fetch_one(&pool)
    .await
    .expect("读会话说");
    let row = Row {
        title: sqlx::Row::try_get::<Option<String>, _>(&record, "title").expect("title"),
        agent_session_id: sqlx::Row::try_get::<Option<String>, _>(&record, "agent_session_id")
            .expect("agent_session_id"),
        workspace_cwd: sqlx::Row::try_get::<Option<String>, _>(&record, "workspace_cwd")
            .expect("workspace_cwd"),
        state: sqlx::Row::try_get::<String, _>(&record, "state").expect("state"),
        mode_id: sqlx::Row::try_get::<Option<String>, _>(&record, "current_mode_id")
            .expect("current_mode_id"),
        mode_name: sqlx::Row::try_get::<Option<String>, _>(&record, "current_mode_name")
            .expect("current_mode_name"),
        workspace_alias: sqlx::Row::try_get::<Option<String>, _>(&record, "workspace_alias")
            .expect("workspace_alias"),
        version: sqlx::Row::try_get::<i64, _>(&record, "version").expect("version"),
        updated_at: sqlx::Row::try_get::<String, _>(&record, "updated_at").expect("updated_at"),
    };
    pool.close().await;
    row
}

fn session_update(title: Option<Option<String>>) -> SessionUpdate {
    session_update_with(title, Some(AGENT_SESSION_ID), Some(WORKSPACE_CWD))
}

/// 与 [`session_update`] 同形，但恢复两列可指定：用来证明「同一提交里的其它列不被吞掉」也覆盖
/// `agent_session_id`/`workspace_cwd`（`None` = 不改该列，用于单独验证不改列的语义）。
fn session_update_with(
    title: Option<Option<String>>,
    agent_session_id: Option<&str>,
    workspace_cwd: Option<&str>,
) -> SessionUpdate {
    SessionUpdate {
        state: Some(SessionState::WaitingInput),
        mode: ModeChange::Set(
            ModeRef::try_new(ModeId::new("code").expect("mode id"), "Code").expect("mode ref"),
        ),
        title,
        closed_at: None,
        agent_session_id: agent_session_id
            .map(|id| acp_core::model::AgentSessionId::new(id).expect("agent session id")),
        workspace_cwd: workspace_cwd.map(str::to_owned),
        workspace_alias: Some("office.main".to_owned()),
        interaction: None,
    }
}

/// `title == Some(Some(text))`：写入标题、版本 +1、`updated_at` 前进，同批其它列一并落盘。
#[tokio::test]
async fn setting_a_title_writes_it_and_bumps_the_version() {
    let dir = temp_dir("title-write-set");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at(0))
        .await
        .expect("open store");
    let created = store
        .commit(create_session(Some("旧标题")))
        .await
        .expect("create");
    let session = created.session_id.clone().expect("session id");

    let outcome = store
        .commit(update(
            &session,
            at(1),
            Some(Version::new(1)),
            Some(StateChange::Update(session_update(Some(Some(
                "新标题".to_owned(),
            ))))),
            vec![mode_changed(2)],
        ))
        .await
        .expect("写入标题");
    assert_eq!(outcome.version, Version::new(2), "含状态变更 ⇒ 版本 +1");

    let row = row(&dir, &session).await;
    assert_eq!(row.title.as_deref(), Some("新标题"));
    assert_eq!(row.version, 2);
    assert_eq!(row.updated_at, at(1).as_str(), "权威更新时间取 commit.at");
    assert_eq!(row.state, "waiting_input");
    assert_eq!(row.mode_id.as_deref(), Some("code"));
    assert_eq!(row.workspace_alias.as_deref(), Some("office.main"));
    store.close().await;
}

/// **F1 的主用例**：一个批次同时含「显式置空标题」与要求注入 `version` 的 `session.mode.changed`。
///
/// 期望（与 SET 路径、以及 `core` 的 fake 端口逐条一致）：
/// 1. 提交**成功**——旧实现返回未递增的版本会让 core 判为版本漂移 → `PortError::Corrupt` 并整批丢事件；
/// 2. `CommitOutcome.version` 恰好是「当前 + 1」，且落盘版本与它相同（core 的比对判据）；
/// 3. `updated_at` 前进到 `commit.at`（目录排序按它，D7）；
/// 4. 同一提交里的 `state`/`current_mode_*`/`workspace_alias` 不被吞掉（旧实现的窄语句丢弃它们）；
/// 5. 该事件的 view 按 §10.3 注入的就是那个递增后的版本，且事件真的落盘。
#[tokio::test]
async fn clearing_a_title_bumps_the_version_and_keeps_the_rest_of_the_batch() {
    let dir = temp_dir("title-write-clear-with-version-event");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at(0))
        .await
        .expect("open store");
    let created = store
        .commit(create_session(Some("将被置空")))
        .await
        .expect("create");
    let session = created.session_id.clone().expect("session id");
    assert_eq!(created.version, Version::new(1));

    let outcome = store
        .commit(update(
            &session,
            at(2),
            None,
            // 显式置空标题，**同时**把状态、模式与目录归属写进同一批。
            Some(StateChange::Update(session_update(Some(None)))),
            // core 会按「含 StateChange ⇒ current + 1 = 2」注入这条事件的 version。
            vec![mode_changed(2)],
        ))
        .await
        .expect("显式置空标题 + 版本事件必须同批提交成功（不得是 Corrupt）");

    assert_eq!(
        outcome.version,
        Version::new(2),
        "F1：显式置空标题同样含 StateChange，版本必须 +1（旧实现返回未递增的旧值）"
    );

    let row = row(&dir, &session).await;
    assert_eq!(row.title, None, "显式置空必须真的写 NULL");
    assert_eq!(row.version, 2, "落盘版本必须与 CommitOutcome 一致");
    assert_eq!(
        row.updated_at,
        at(2).as_str(),
        "F1：updated_at 必须前进到 commit.at（旧实现不写该列）"
    );
    // 同批的其它列——旧实现的窄语句把这一整组静默丢弃。
    assert_eq!(row.state, "waiting_input", "同批的 state 不得被吞掉");
    assert_eq!(
        row.agent_session_id.as_deref(),
        Some(AGENT_SESSION_ID),
        "同批的 agent_session_id 不得被吞掉"
    );
    assert_eq!(
        row.workspace_cwd.as_deref(),
        Some(WORKSPACE_CWD),
        "同批的 workspace_cwd 不得被吞掉"
    );
    assert_eq!(
        row.mode_id.as_deref(),
        Some("code"),
        "同批的模式列不得被吞掉"
    );
    assert_eq!(row.mode_name.as_deref(), Some("Code"));
    assert_eq!(
        row.workspace_alias.as_deref(),
        Some("office.main"),
        "同批的目录归属列不得被吞掉"
    );

    // 事件真的落盘，且 view 里注入的正是递增后的版本（core 的漂移检测比对的就是这个值）。
    let event_id = outcome.appended[0].id.clone();
    let payload = store
        .read_view()
        .await
        .expect("view")
        .event_payload(&event_id)
        .await
        .expect("payload")
        .expect("事件必须已落盘");
    assert_eq!(
        payload.view.as_str(),
        r#"{"version":"2","currentModeId":"code"}"#,
        "该事件必须按递增后的会话版本注入（§10.3）"
    );

    store.close().await;
}

/// 判定力对照：`title == None`（不改该列）不得清掉既有标题，版本仍按含 `StateChange` 递增。
///
/// 这条守住「`None` 与 `Some(None)` 不可混同」——两层可选的另一半。
#[tokio::test]
async fn an_unchanged_title_keeps_the_existing_value_and_still_bumps_the_version() {
    let dir = temp_dir("title-write-unchanged");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at(0))
        .await
        .expect("open store");
    let created = store
        .commit(create_session(Some("保留我")))
        .await
        .expect("create");
    let session = created.session_id.clone().expect("session id");

    let outcome = store
        .commit(update(
            &session,
            at(1),
            Some(Version::new(1)),
            Some(StateChange::Update(session_update(None))),
            vec![mode_changed(2)],
        ))
        .await
        .expect("不改标题");
    assert_eq!(outcome.version, Version::new(2));
    let row = row(&dir, &session).await;
    assert_eq!(row.title.as_deref(), Some("保留我"), "None = 不改该列");
    assert_eq!(row.updated_at, at(1).as_str());
    store.close().await;
}

/// **旧缺陷的第二种形态**：`ModeChange::Set`（bind 编号是另一套）下的显式置空也走同一条语句。
///
/// `Set` 的语句里标题哨兵是 `?11`、写入值是 `?10`；若按 `Unchanged` 的编号绑定，置空会落到别的参数上
/// （静默不写、或写错列），本用例用「模式列与新标题同时可见」把编号绑定的正确性一并钉住。
#[tokio::test]
async fn clearing_a_title_with_a_mode_change_writes_both_columns() {
    let dir = temp_dir("title-write-clear-with-mode-change");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at(0))
        .await
        .expect("open store");
    let created = store
        .commit(create_session(Some("旧标题")))
        .await
        .expect("create");
    let session = created.session_id.clone().expect("session id");

    let outcome = store
        .commit(update(
            &session,
            at(3),
            None,
            Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Set(
                    ModeRef::try_new(ModeId::new("plan").expect("mode id"), "Plan")
                        .expect("mode ref"),
                ),
                title: Some(None),
                closed_at: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_alias: None,
                interaction: None,
            })),
            vec![mode_changed(2)],
        ))
        .await
        .expect("置空 + 模式变更同批");
    assert_eq!(outcome.version, Version::new(2));

    let row = row(&dir, &session).await;
    assert_eq!(row.title, None, "置空必须真的写 NULL");
    assert_eq!(row.mode_id.as_deref(), Some("plan"), "模式列必须真的落盘");
    assert_eq!(row.mode_name.as_deref(), Some("Plan"));
    assert_eq!(row.version, 2);
    assert_eq!(row.updated_at, at(3).as_str());
    store.close().await;
}
