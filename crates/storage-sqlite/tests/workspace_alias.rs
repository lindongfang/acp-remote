//! `storage-sqlite` 的目录归属列 `owned_session.workspace_directory_id`：写入、读回、v8→v9 升级与投影。
//!
//! 权威是 `docs/CORE_PORTS_AND_STORAGE.md` §7.2/§7.3 与变更 `directory-auto-registration` 的
//! `storage-schema-v2-migration` / `workspace-resolution` 增量。本文件覆盖目录归属列自己的维度：
//!
//! - **v8 → v9 升级**：既有 `owned_workspace` 行与 `workspace_alias` 回填成目录标识；别名相同路径的
//!   多行合并；`workspace_alias` 为空但 `workspace_cwd` 非空的会话被归组并写入派生标识；旧表删除；
//! - **往返**：写入的目录标识**按字节**读回，没写过的行读回 `None`；
//! - **不按路径反查**：`workspace_cwd` 恰好等于某个已记录目录的规范化路径、而标识是 `NULL` 的会话，
//!   仍然是未分组；
//! - **恢复不改写**：窄读取前后该列逐字节不变；
//! - **模式变更同批写入**：`ModeChange::Set` 与非空标识同批提交时，模式与标识都真的落盘。
//!
//! 全部用例只经真实 SQLite 文件与 `storage_sqlite::migrate` 打开路径取证（`sqlite_master` / `quote()` /
//! `PRAGMA table_info` / 端口），断言读的是库文件里的字节。

mod support;

use acp_core::model::{
    AgentSessionId, DirectoryId, ModeId, ModeRef, OriginEpoch, Timestamp, derive_directory_id,
};
use acp_core::ports::{
    LocalConfigStore, ModeChange, NewSession, OwnedCommit, SessionQuery, SessionStore,
    SessionUpdate, StateChange,
};
use storage_sqlite::migrate::{
    DATABASE_FILE, FILE_FORMAT_VERSION, OWNED_SCHEMA_VERSION, StorageConfig,
};
use storage_sqlite::session_store::SqliteStore;
use support::*;

const AT: &str = "2026-09-18T00:00:00.000Z";
const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const SESSION_OTHER: &str = "11111111-1111-4111-8111-111111111112";
const EPOCH: &str = "33333333-3333-4333-8333-333333333333";
const ALIAS: &str = "acp.remote";
const ALIAS_OTHER: &str = "office.main";

fn at() -> Timestamp {
    Timestamp::new(AT).expect("规范时间戳")
}

/// 平台无关的绝对路径（Windows 上 `/srv/...` 不是绝对路径）。
fn absolute_path(name: &str) -> String {
    std::env::temp_dir()
        .join("acpr-storage-directory")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn directory_id(path: &str) -> DirectoryId {
    derive_directory_id(path)
}

/// 把当前版本库回退成 **v8 形状**（目录归属列改回 `workspace_alias`、恢复 `owned_workspace`、删掉
/// `owned_directory`），返回库文件路径。
async fn rewind_to_v8(dir: &std::path::Path) -> std::path::PathBuf {
    let store = SqliteStore::open(StorageConfig::new(dir), &at())
        .await
        .expect("先建一个当前版本库再回退");
    store.close().await;
    let path = dir.join(DATABASE_FILE);
    let pool = raw_write_pool(&path).await;
    sqlx::raw_sql(
        "ALTER TABLE owned_session RENAME COLUMN workspace_directory_id TO workspace_alias;
         CREATE TABLE owned_workspace (
             alias          TEXT PRIMARY KEY,
             display_name   TEXT NOT NULL,
             canonical_path TEXT NOT NULL,
             created_at     TEXT NOT NULL,
             updated_at     TEXT NOT NULL
         ) STRICT;
         DROP TABLE owned_directory;",
    )
    .execute(&pool)
    .await
    .expect("回退成 v8 形状");
    sqlx::query("PRAGMA user_version = 8")
        .execute(&pool)
        .await
        .expect("回退文件格式版本");
    sqlx::query("UPDATE meta SET value = '8' WHERE key = 'owned_schema_version'")
        .execute(&pool)
        .await
        .expect("回退 owned 家族表结构版本");
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .expect("checkpoint");
    pool.close().await;
    path
}

/// 直接写一行 v8 形状的 owned 会话。
async fn seed_v8_session(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    workspace_cwd: Option<&str>,
    workspace_alias: Option<&str>,
) {
    sqlx::query(
        r#"INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, origin_epoch,
         current_mode_id, current_mode_name, version, created_at, updated_at, closed_at,
         agent_session_id, workspace_cwd, workspace_alias)
         VALUES (?1, 'seeded', 'fixture-agent', 'Fixture Agent', 'idle', ?2, NULL, NULL, 1, ?3, ?3,
         NULL, NULL, ?4, ?5)"#,
    )
    .bind(session_id)
    .bind(EPOCH)
    .bind(AT)
    .bind(workspace_cwd)
    .bind(workspace_alias)
    .execute(pool)
    .await
    .expect("种下 v8 会话行");
}

async fn seed_v8_workspace(pool: &sqlx::SqlitePool, alias: &str, path: &str) {
    sqlx::query(
        "INSERT INTO owned_workspace (alias, display_name, canonical_path, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?4)",
    )
    .bind(alias)
    .bind(alias)
    .bind(path)
    .bind(AT)
    .execute(pool)
    .await
    .expect("种下 v8 目录行");
}

async fn quoted_directory(pool: &sqlx::SqlitePool, session_id: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT quote(workspace_directory_id) FROM owned_session WHERE session_id = ?1",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .expect("读 workspace_directory_id 的原始字节")
}

/// 一条最简的会话创建提交（`session_id` 由存储层在事务内分配）。
fn create_session(title: &str) -> OwnedCommit {
    OwnedCommit {
        session: None,
        at: at(),
        expected_version: None,
        state: Some(StateChange::Create(NewSession {
            title: Some(title.to_owned()),
            agent: acp_core::model::AgentRef::try_new(
                acp_core::model::AgentId::new("probe-agent").expect("agent id"),
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
        origin_epoch: Some(OriginEpoch::new(EPOCH).expect("origin epoch")),
    }
}

/// **v8 → v9**：既有目录行与归属回填成标识，旧表删除，升级后连续两次打开 schema 文本不变。
#[tokio::test]
async fn v8_database_upgrades_to_v9_and_backfills_directory_ids() {
    let dir = temp_dir("v8-to-v9-directories");
    let path = rewind_to_v8(&dir).await;
    let shared = absolute_path("shared");
    let orphan = absolute_path("orphan");
    let pool = raw_write_pool(&path).await;
    seed_v8_workspace(&pool, ALIAS, &shared).await;
    seed_v8_workspace(&pool, ALIAS_OTHER, &shared).await;
    seed_v8_session(&pool, SESSION, Some(&shared), Some(ALIAS)).await;
    seed_v8_session(&pool, SESSION_OTHER, Some(&orphan), None).await;
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("升级到 v9");
    let directories = store.directories().await.expect("读取目录表");
    assert_eq!(directories.len(), 2, "同路径的两个别名合并为一条目录行");
    let shared_id = directory_id(&shared);
    let orphan_id = directory_id(&orphan);
    assert!(
        directories
            .iter()
            .any(|record| record.directory_id() == &shared_id)
    );
    assert!(
        directories
            .iter()
            .any(|record| record.directory_id() == &orphan_id)
    );

    let summaries = store
        .list(SessionQuery {
            only: None,
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("读取会话列表");
    assert_eq!(
        summaries
            .iter()
            .find(|summary| summary.session_id().as_str() == SESSION)
            .and_then(|summary| summary.workspace())
            .map(|workspace| workspace.directory_id().clone()),
        Some(shared_id)
    );
    assert_eq!(
        summaries
            .iter()
            .find(|summary| summary.session_id().as_str() == SESSION_OTHER)
            .and_then(|summary| summary.workspace())
            .map(|workspace| workspace.directory_id().clone()),
        Some(orphan_id),
        "空别名但 cwd 非空的会话也被归组"
    );
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    assert_eq!(
        meta_rows(&pool)
            .await
            .iter()
            .find(|(key, _)| key == "owned_schema_version")
            .map(|(_, value)| value.as_str()),
        Some(OWNED_SCHEMA_VERSION.to_string().as_str())
    );
    assert!(
        !schema_sql(&pool)
            .await
            .iter()
            .any(|(kind, name, _)| kind == "table" && name == "owned_workspace"),
        "v9 必须删除 owned_workspace"
    );
    assert!(
        schema_sql(&pool)
            .await
            .iter()
            .any(|(kind, name, _)| kind == "table" && name == "owned_directory")
    );
    pool.close().await;

    // 连续两次打开不重写任何 schema 文本。
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("第二次打开");
    store.close().await;
    let first = {
        let pool = raw_pool(&path).await;
        let schema = schema_sql(&pool).await;
        pool.close().await;
        schema
    };
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("第三次打开");
    store.close().await;
    let second = {
        let pool = raw_pool(&path).await;
        let schema = schema_sql(&pool).await;
        pool.close().await;
        schema
    };
    assert_eq!(first, second, "连续两次打开的 schema 文本必须逐字节相同");
}

/// 写入的标识**按字节**读回；没写过的会话读回 `None`。
#[tokio::test]
async fn the_written_directory_id_round_trips_verbatim_and_an_unwritten_row_stays_none() {
    let dir = temp_dir("directory-round-trip");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("新建库");
    let path = dir.join(DATABASE_FILE);
    let session = store
        .commit(create_session("round-trip"))
        .await
        .expect("创建会话")
        .session_id
        .expect("分配的 sessionId");
    let identifier = directory_id(&absolute_path("round-trip"));
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                title: None,
                closed_at: None,
                interaction: None,
                agent_session_id: None,
                workspace_cwd: Some(absolute_path("round-trip")),
                workspace_directory_id: Some(identifier.as_str().to_owned()),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        })
        .await
        .expect("写入目录标识");
    store.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    let workspace = store
        .list(SessionQuery {
            only: Some(vec![session.clone()]),
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("列表")[0]
        .workspace()
        .expect("目录标识必须读回")
        .clone();
    assert_eq!(workspace.directory_id(), &identifier);
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        quoted_directory(&pool, session.as_str()).await,
        format!("'{identifier}'")
    );
    pool.close().await;
}

/// `ModeChange::Set` 与非空目录标识同批提交时，两侧都真的落盘（bind 编号错位会静默吞掉一侧）。
#[tokio::test]
async fn a_mode_change_commit_also_persists_the_workspace_directory_id() {
    let dir = temp_dir("directory-set-mode");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("新建库");
    let path = dir.join(DATABASE_FILE);
    let session = store
        .commit(create_session("set-mode"))
        .await
        .expect("创建会话")
        .session_id
        .expect("分配的 sessionId");
    let identifier = directory_id(&absolute_path("set-mode"));
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Set(
                    ModeRef::try_new(ModeId::new("code").expect("mode id"), "Code")
                        .expect("mode ref"),
                ),
                title: None,
                closed_at: None,
                interaction: None,
                agent_session_id: Some(AgentSessionId::new("acp-session-1").expect("会话标识")),
                workspace_cwd: Some(absolute_path("set-mode")),
                workspace_directory_id: Some(identifier.as_str().to_owned()),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        })
        .await
        .expect("同批写入模式与目录标识");
    store.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    let snapshot = store.load(&session).await.expect("读取").expect("会话存在");
    assert_eq!(
        snapshot
            .session
            .current_mode()
            .map(|mode| mode.mode_id().as_str()),
        Some("code")
    );
    let workspace = store
        .list(SessionQuery {
            only: Some(vec![session.clone()]),
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("列表")[0]
        .workspace()
        .expect("同批提交的标识必须读回")
        .clone();
    assert_eq!(workspace.directory_id(), &identifier);
    let recovery = store
        .load_recovery(&session)
        .await
        .expect("窄读取")
        .expect("两列都已写入");
    assert_eq!(
        recovery
            .agent_session_id
            .as_ref()
            .map(AgentSessionId::as_str),
        Some("acp-session-1")
    );
    assert_eq!(
        recovery.workspace_cwd.as_deref(),
        Some(absolute_path("set-mode").as_str())
    );
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        quoted_directory(&pool, session.as_str()).await,
        format!("'{identifier}'")
    );
    pool.close().await;
}

/// `workspace_cwd` 恰好等于某个已记录目录的规范化路径、而标识是 `NULL` 的会话仍然是未分组。
#[tokio::test]
async fn a_null_directory_id_is_never_back_filled_from_the_canonical_path() {
    let dir = temp_dir("directory-null");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("新建库");
    let path = dir.join(DATABASE_FILE);
    let session = store
        .commit(create_session("null-value"))
        .await
        .expect("创建会话")
        .session_id
        .expect("分配的 sessionId");
    let cwd = absolute_path("null-value");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                title: None,
                closed_at: None,
                interaction: None,
                agent_session_id: None,
                workspace_cwd: Some(cwd.clone()),
                workspace_directory_id: None,
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        })
        .await
        .expect("只写 cwd");
    store.close().await;

    // 库里存在一条路径相同的目录记录——但读取路径不得据此补出归属。
    let pool = raw_write_pool(&path).await;
    sqlx::query(
        "INSERT INTO owned_directory (directory_id, canonical_path, created_at, last_used_at) \
         VALUES (?1, ?2, ?3, ?3)",
    )
    .bind(directory_id(&cwd).as_str())
    .bind(&cwd)
    .bind(AT)
    .execute(&pool)
    .await
    .expect("插入目录行");
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    let workspace = store
        .list(SessionQuery {
            only: Some(vec![session.clone()]),
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("列表")[0]
        .workspace()
        .cloned();
    assert_eq!(workspace, None, "NULL 不得按路径反查补齐");
    store.close().await;
}

/// 恢复流程（窄读取）前后该列的字节相同。
#[tokio::test]
async fn the_resume_flow_neither_reads_nor_rewrites_the_directory_id() {
    let dir = temp_dir("directory-resume");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("新建库");
    let path = dir.join(DATABASE_FILE);
    let session = store
        .commit(create_session("resume"))
        .await
        .expect("创建会话")
        .session_id
        .expect("分配的 sessionId");
    let identifier = directory_id(&absolute_path("resume"));
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                title: None,
                closed_at: None,
                interaction: None,
                agent_session_id: Some(AgentSessionId::new("acp-session-resume").expect("id")),
                workspace_cwd: Some(absolute_path("resume")),
                workspace_directory_id: Some(identifier.as_str().to_owned()),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: None,
        })
        .await
        .expect("写入目录标识");
    let before = {
        let pool = raw_pool(&path).await;
        let value = quoted_directory(&pool, session.as_str()).await;
        pool.close().await;
        value
    };
    let recovery = store
        .load_recovery(&session)
        .await
        .expect("窄读取")
        .expect("恢复数据");
    assert!(recovery.workspace_cwd.is_some());
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(quoted_directory(&pool, session.as_str()).await, before);
    pool.close().await;
}
