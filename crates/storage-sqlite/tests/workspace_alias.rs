//! `storage-sqlite` 的 v6 目录归属列 `owned_session.workspace_alias`：写入、读回与投影。
//!
//! 权威是 `docs/CORE_PORTS_AND_STORAGE.md` §7.2/§7.3 与 `openspec/changes/sync-workspaces-and-create/`
//! 的 `storage-schema-v2-migration` / `workspace-resolution` 增量（`design.md` D1/D6）。本文件**不**复述
//! `migration.rs` 已有的 v1 → v6 连续升级保留性与 `resume_columns.rs` 的恢复两列判据，只补目录归属列
//! 自己的维度：
//!
//! - **v5 → v6 升级**：既有会话行的其余列逐字节不变、`workspace_alias` 是真正的 `NULL`（不是空串、
//!   不是占位别名），且这些会话在摘要里是**未分组**；
//! - **往返**：写入的别名**按字节**读回（不规范化、不按路径重解析），没写过的行读回 `None`；
//! - **不按路径反查**：`workspace_cwd` 恰好等于某个已登记目录的规范化路径、而别名是 `NULL` 的会话，
//!   仍然是未分组；
//! - **展示名解析**：别名已登记 → 取 `owned_workspace.display_name`；别名不再登记 → 回退为别名本身；
//!   同一别名重指向别的目录 → 既有会话的归属不变；
//! - **恢复不改写**：走一遍恢复流程的存储交互后该列逐字节不变。
//! - **模式变更同批写入**：`ModeChange::Set`（它用 `?5/?6/?7` 之后的**另一套**编号）与非空别名同批
//!   提交时，模式与别名都真的落盘（bind 编号错位会让其中一侧被静默吞掉）。
//!
//! 全部用例只经真实 SQLite 文件与 `storage_sqlite::migrate` 打开路径取证（`sqlite_master` / `quote()` /
//! `PRAGMA table_info` / 端口），断言读的是库文件里的字节。

mod support;

use acp_core::model::{
    AgentSessionId, ModeId, ModeRef, OriginEpoch, Timestamp, WorkspaceAlias, WorkspaceRecord,
};
use acp_core::ports::{
    ModeChange, NewSession, OwnedCommit, SessionQuery, SessionStore, SessionUpdate, StateChange,
};
use storage_sqlite::migrate::{
    DATABASE_FILE, FILE_FORMAT_VERSION, OWNED_SCHEMA_VERSION, StorageConfig,
};
use storage_sqlite::session_store::SqliteStore;
use support::*;

/// 本文件的固定时间戳（`storage-sqlite` 不读系统时钟）。
const AT: &str = "2026-09-18T00:00:00.000Z";
/// 用例一的会话标识。
const SESSION: &str = "11111111-1111-4111-8111-111111111111";
/// 用例一的 origin epoch。
const EPOCH: &str = "33333333-3333-4333-8333-333333333333";
/// 别名原文：`^[a-z0-9][a-z0-9._-]{0,63}$`（`core::model::WorkspaceAlias`）。
const ALIAS: &str = "acp.remote";
/// 第二个别名，用来证明归属按**别名**而不是按路径或行序解析。
const ALIAS_OTHER: &str = "office.main";

fn at() -> Timestamp {
    Timestamp::new(AT).expect("规范时间戳")
}

/// 把当前版本库回退成 **v5 形状**（`DROP` 掉 v6 才有的 `workspace_alias` 列、降两个版本号），
/// 返回库文件路径。
///
/// 与 `migration.rs` / `resume_columns.rs` 的回退手法同源（`DROP COLUMN` + `PRAGMA user_version` +
/// `meta` 回写 + `wal_checkpoint(TRUNCATE)`），**不新增 `fixtures/storage/v5/` 夹具**。
async fn rewind_to_v5(dir: &std::path::Path) -> std::path::PathBuf {
    let store = SqliteStore::open(StorageConfig::new(dir), &at())
        .await
        .expect("先建一个当前版本库再回退");
    store.close().await;
    let path = dir.join(DATABASE_FILE);
    let pool = raw_write_pool(&path).await;
    sqlx::query("ALTER TABLE owned_session DROP COLUMN workspace_alias")
        .execute(&pool)
        .await
        .expect("回退 owned_session 的目录归属列");
    sqlx::query("PRAGMA user_version = 5")
        .execute(&pool)
        .await
        .expect("回退文件格式版本");
    sqlx::query("UPDATE meta SET value = '5' WHERE key = 'owned_schema_version'")
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

/// 直接写一行 owned 会话（绕开端口：这几条要的是**库内字节形状**，不是提交语义）。
async fn seed_session(pool: &sqlx::SqlitePool, session_id: &str, workspace_cwd: Option<&str>) {
    sqlx::query(
        r#"INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, origin_epoch,
         current_mode_id, current_mode_name, version, created_at, updated_at, closed_at,
         agent_session_id, workspace_cwd)
         VALUES (?1, 'seeded', 'fixture-agent', 'Fixture Agent', 'idle', ?2, NULL, NULL, 1, ?3, ?3,
         NULL, NULL, ?4)"#,
    )
    .bind(session_id)
    .bind(EPOCH)
    .bind(AT)
    .bind(workspace_cwd)
    .execute(pool)
    .await
    .expect("种下会话行");
}

/// 平台无关的绝对路径（`WorkspaceRecord` 只接受绝对路径形状；Windows 上 `/srv/...` 不是绝对路径，
/// 与 `admin_store.rs` 的同名辅助同款）。
fn absolute_path(name: &str) -> String {
    std::env::temp_dir()
        .join("acpr-storage-workspace-alias")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// 经端口登记一个目录（`owned_workspace`）：别名、展示名与**平台无关的**规范化路径（由 `name` 派生）。
async fn register_workspace(pool: &sqlx::SqlitePool, alias: &str, display_name: &str, name: &str) {
    let record = WorkspaceRecord::try_new(
        WorkspaceAlias::new(alias).expect("别名"),
        display_name,
        &absolute_path(name),
        at(),
        at(),
    )
    .expect("目录记录");
    sqlx::query(
        "INSERT INTO owned_workspace (alias, display_name, canonical_path, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(record.alias().as_str())
    .bind(record.display_name())
    .bind(record.canonical_path())
    .bind(AT)
    .bind(AT)
    .execute(pool)
    .await
    .expect("登记目录");
}

/// 列库里 `workspace_alias` 的原始字节（`quote()` 区分 `NULL` 与空串）。
async fn quoted_alias(pool: &sqlx::SqlitePool, session_id: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT quote(workspace_alias) FROM owned_session WHERE session_id = ?1",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .expect("读 workspace_alias 的原始字节")
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

// ---------------------------------------------------------------------------------------------
// v5 → v6 升级
// ---------------------------------------------------------------------------------------------

/// **v5 → v6 升级**：既有会话行的其余列逐字节不变、`workspace_alias` 是 `NULL`，且这些会话被当作
/// **未分组**（摘要的 `workspace` 是 `None`）。
///
/// 判别力：若升级段做了反查补齐（按 `workspace_cwd` 找别名）或写了空串/占位别名，第 ①/② 条断言失败；
/// 若它重写了表（12-step 重建），第 ③ 条的「既有列定义文本逐字节保留」失败；`workspace` 断言则要求
/// 读取路径真的读这一列，而不是硬编码 `None`。
#[tokio::test]
async fn v5_database_upgrades_to_v6_by_appending_a_null_workspace_alias_column() {
    let dir = temp_dir("tp4-v5-to-v6");
    let path = rewind_to_v5(&dir).await;

    // 造 v5 库：一条**带恢复数据**的真实会话行——这样「其余列逐字节不变」是有内容的断言。
    let pool = raw_write_pool(&path).await;
    sqlx::query(
        r#"INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, origin_epoch,
         current_mode_id, current_mode_name, version, created_at, updated_at, closed_at,
         agent_session_id, workspace_cwd)
         VALUES (?1, 'kept session', 'fixture-agent', 'Fixture Agent', 'waiting_input', ?2, 'code',
         'Code', 7, ?3, ?3, NULL, 'acp-session-0001', ?4)"#,
    )
    .bind(SESSION)
    .bind(EPOCH)
    .bind(AT)
    .bind(absolute_path("probe"))
    .execute(&pool)
    .await
    .expect("种下 v5 形状的会话行");
    // 该目录**确实已登记**，且规范化路径与会话行的 `workspace_cwd` 相同——若实现反查补齐，这里就会
    // 造出一个本不该有的归属。
    register_workspace(&pool, ALIAS, "Acp Remote", "probe").await;

    assert_eq!(scalar_i64(&pool, "PRAGMA user_version").await, 5);
    assert!(
        !column_names(&pool, "owned_session")
            .await
            .contains(&"workspace_alias".to_owned()),
        "回退出来的库必须真的没有 workspace_alias 列"
    );
    let ddl_before: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'owned_session'",
    )
    .fetch_one(&pool)
    .await
    .expect("读 v5 的 owned_session DDL");
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("a v5 database must upgrade to v6");
    assert_eq!(store.metadata().owned_schema_version, 6);
    assert_eq!(store.metadata().imported_schema_version, 3);
    let summaries = store
        .list(SessionQuery {
            only: None,
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("list upgraded sessions");
    assert_eq!(summaries.len(), 1);
    assert_eq!(
        summaries[0].workspace(),
        None,
        "升级前的会话没有目录归属，升级后也必须是未分组（不得反查 workspace_cwd 补齐）"
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
            .into_iter()
            .find(|(key, _)| key == "owned_schema_version")
            .map(|(_, value)| value),
        Some(OWNED_SCHEMA_VERSION.to_string())
    );

    // ① 其余列逐字节不变（包含 v5 已有的两列——升级不得动它们）。
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            r#"SELECT session_id || '|' || COALESCE(title, '∅') || '|' || agent_id || '|' ||
               agent_name || '|' || state || '|' || origin_epoch || '|' ||
               COALESCE(current_mode_id, '∅') || '|' || COALESCE(current_mode_name, '∅') || '|' ||
               version || '|' || created_at || '|' || updated_at || '|' ||
               COALESCE(closed_at, '∅') || '|' || COALESCE(agent_session_id, '∅') || '|' ||
               COALESCE(workspace_cwd, '∅') FROM owned_session"#,
        )
        .fetch_one(&pool)
        .await
        .expect("读升级后的会话行"),
        format!(
            "{SESSION}|kept session|fixture-agent|Fixture Agent|waiting_input|{EPOCH}|code|Code|7|\
             {AT}|{AT}|∅|acp-session-0001|{}",
            absolute_path("probe")
        ),
        "升级不得改动既有会话行的任何一列"
    );

    // ② 新列是真正的 `NULL`（`quote()` 会把 `NULL` 渲染成 `NULL`、空串渲染成 `''`）。
    assert_eq!(
        quoted_alias(&pool, SESSION).await,
        "NULL",
        "既有会话行的 workspace_alias 必须是 NULL——不是空串，也不是任何占位别名"
    );

    // ③ 只追加、不重建：既有列的定义文本逐字节保留，新列紧跟其后。
    let ddl_after: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'owned_session'",
    )
    .fetch_one(&pool)
    .await
    .expect("读 v6 的 owned_session DDL");
    let head = ddl_before
        .rfind(')')
        .map(|cut| ddl_before[..cut].trim_end().trim_end_matches(','))
        .unwrap_or_else(|| panic!("v5 文本形状异常（找不到表尾右括号）：{ddl_before}"));
    assert!(
        ddl_after.contains(head),
        "既有列的定义文本必须逐字节保留（重建会重排空白与排版）：\n{head}\n{ddl_after}"
    );
    assert!(
        ddl_after.contains(&format!("{head}, workspace_alias TEXT)")),
        "新列必须紧跟既有列追加：{ddl_after}"
    );
    assert!(
        ddl_after.starts_with("CREATE TABLE owned_session ("),
        "追加路径的存储文本不带引号表名；12-step 重建会写成 \"owned_session\"：{ddl_after}"
    );
    let specs = column_specs(&pool, "owned_session").await;
    assert_eq!(specs.len(), 15, "v6 的 owned_session 是 15 列");
    assert_eq!(specs[14].0, "workspace_alias");
    assert_eq!(
        (&specs[14].1, specs[14].2, &specs[14].3),
        (&"TEXT".to_owned(), false, &None),
        "workspace_alias 必须可空、无类型修饰且无默认值（空串做不到这一点）"
    );
    pool.close().await;
}

// ---------------------------------------------------------------------------------------------
// 往返
// ---------------------------------------------------------------------------------------------

/// 写入的别名**按字节**读回（关闭后重开一个 store 实例）；没写过的会话读回 `None`。
#[tokio::test]
async fn the_written_alias_round_trips_verbatim_and_an_unwritten_row_stays_none() {
    let dir = temp_dir("tp4-round-trip");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("新建库");
    let path = dir.join(DATABASE_FILE);

    let session = store
        .commit(create_session("grouped"))
        .await
        .expect("创建会话")
        .session_id
        .expect("分配的 sessionId");
    let ungrouped = store
        .commit(create_session("ungrouped"))
        .await
        .expect("创建第二条会话")
        .session_id
        .expect("分配的 sessionId");

    // 会话创建后紧随其后的那一次窄写：别名按**原文**写入。
    let written = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
                agent_session_id: Some(AgentSessionId::new("acp-session-1").expect("会话标识")),
                // 一个**非规范化形状**的路径，用来证明存储层不对它做任何加工。
                workspace_cwd: Some(format!("{}/./nested/", absolute_path("demo"))),
                workspace_alias: Some(ALIAS.to_owned()),
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
        .expect("写入目录归属");
    assert_eq!(written.version.get(), 2, "写入会推进会话版本");
    store.close().await;

    // 重开一个**新的 store 实例**：读回必须来自库文件，而不是进程内缓存。
    let reopened = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    let pool = raw_write_pool(&path).await;
    register_workspace(&pool, ALIAS, "Acp Remote", "demo-nested").await;
    pool.close().await;

    let summaries = reopened
        .list(SessionQuery {
            only: Some(vec![session.clone()]),
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("读取分组会话");
    let workspace = summaries[0]
        .workspace()
        .expect("写入过的别名必须读回")
        .clone();
    assert_eq!(workspace.alias().as_str(), ALIAS, "别名按写入的原文读回");
    assert_eq!(workspace.display_name(), "Acp Remote");

    assert_eq!(
        reopened
            .list(SessionQuery {
                only: Some(vec![ungrouped.clone()]),
                states: Vec::new(),
                limit: None,
            })
            .await
            .expect("读取未分组会话")[0]
            .workspace(),
        None,
        "没有写入过别名的会话读回仍是 None（不得出现空串、别名猜测或占位路径）"
    );
    reopened.close().await;

    // 库文件字节层面的复核。
    let pool = raw_pool(&path).await;
    assert_eq!(
        quoted_alias(&pool, session.as_str()).await,
        format!("'{ALIAS}'"),
        "库内的别名必须与写入的字节相同"
    );
    assert_eq!(
        quoted_alias(&pool, ungrouped.as_str()).await,
        "NULL",
        "未写入的行必须保持 NULL"
    );
    pool.close().await;
}

// ---------------------------------------------------------------------------------------------
// 模式变更与目录归属同批写入
// ---------------------------------------------------------------------------------------------

/// **同一条** `SessionUpdate` 里既 `ModeChange::Set(..)` 又带 `workspace_alias` 时，两侧都真的落盘：
/// 模式经 `load` 读回、别名经摘要投影读回，且库内字节与写入的原文相同。
///
/// 判别力：`ModeChange::Set` 变体的 UPDATE 用的是**另一套** bind 编号（`?5/?6` 是模式，
/// `?7/?8/?9` 才是恢复两列与目录归属列），而 `Unchanged` 变体是 `?5/?6/?7`。两条语句各自独立
/// 编号，一旦某条被整体错位，SQLite 会把绑定落到**别的列**上（或让 `?N IS NULL` 恒真而静默不写）
/// ——全仓其余用例都用 `workspace_alias: None` 走这条路，覆盖不到「`Set` + 非空别名」这一组合。
/// 本用例是它的唯一防线：模式与别名读回断言任一失败，即说明编号已经错位。
#[tokio::test]
async fn a_mode_change_commit_also_persists_the_workspace_alias() {
    let dir = temp_dir("tp4-set-mode-with-alias");
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

    let written = store
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
                closed_at: None,
                interaction: None,
                agent_session_id: Some(AgentSessionId::new("acp-session-1").expect("会话标识")),
                workspace_cwd: Some(absolute_path("set-mode")),
                workspace_alias: Some(ALIAS.to_owned()),
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
        .expect("同批写入模式与目录归属");
    assert_eq!(written.version.get(), 2, "含状态变更的提交推进会话版本");
    store.close().await;

    let pool = raw_write_pool(&path).await;
    register_workspace(&pool, ALIAS, "Acp Remote", "set-mode").await;
    pool.close().await;

    // ① 模式变更真的落盘（`load` 的窄读取路径）。
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    let snapshot = store
        .load(&session)
        .await
        .expect("读取会话")
        .expect("会话存在");
    let mode = snapshot
        .session
        .current_mode()
        .expect("`ModeChange::Set` 必须写入 current_mode_id/current_mode_name");
    assert_eq!(mode.mode_id().as_str(), "code");
    assert_eq!(mode.display_name(), "Code");

    // ② 目录归属真的落盘（摘要投影路径）。
    let workspace = store
        .list(SessionQuery {
            only: Some(vec![session.clone()]),
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("读取会话列表")[0]
        .workspace()
        .expect("同批提交的别名必须读回")
        .clone();
    assert_eq!(
        workspace.alias().as_str(),
        ALIAS,
        "`ModeChange::Set` 变体下别名不得被 bind 编号错位吞掉"
    );
    assert_eq!(workspace.display_name(), "Acp Remote");

    // ③ 恢复两列同样落在这条语句上：它们与别名共用 `Set` 变体的编号，一并复核。
    let record = store
        .load_recovery(&session)
        .await
        .expect("窄读取")
        .expect("两列都已写入");
    assert_eq!(
        record.agent_session_id.as_ref().map(AgentSessionId::as_str),
        Some("acp-session-1"),
        "`agent_session_id` 必须落到自己的位置，而不是被相邻编号挪走"
    );
    assert_eq!(
        record.workspace_cwd.as_deref(),
        Some(absolute_path("set-mode").as_str())
    );
    store.close().await;

    // ④ 库文件字节层面的复核：别名是写入的原文，不是空串、不是 `NULL`、也不是相邻列的值。
    let pool = raw_pool(&path).await;
    assert_eq!(
        quoted_alias(&pool, session.as_str()).await,
        format!("'{ALIAS}'")
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT quote(current_mode_id) FROM owned_session WHERE session_id = ?1",
        )
        .bind(session.as_str())
        .fetch_one(&pool)
        .await
        .expect("读 current_mode_id"),
        "'code'"
    );
    pool.close().await;
}

// ---------------------------------------------------------------------------------------------
// 不按路径反查
// ---------------------------------------------------------------------------------------------

/// `workspace_cwd` 恰好等于某个**已登记**目录的规范化路径、而 `workspace_alias` 是 `NULL` 的会话，
/// 投影仍然是**未分组**。
///
/// 这是本文件最重要的判别式：实现里只要出现「`LEFT JOIN owned_workspace ON canonical_path =
/// workspace_cwd`」这类反查，本用例就失败（`workspace` 会是 `Some`）。
#[tokio::test]
async fn a_null_alias_is_never_back_filled_from_the_canonical_path() {
    let dir = temp_dir("tp4-no-reverse-lookup");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("新建库");
    let path = dir.join(DATABASE_FILE);
    store.close().await;

    let pool = raw_write_pool(&path).await;
    // 目录已登记，且规范化路径与会话行的 `workspace_cwd` **完全相同**。
    register_workspace(&pool, ALIAS, "Acp Remote", "probe").await;
    seed_session(&pool, SESSION, Some(&absolute_path("probe"))).await;
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    let summaries = store
        .list(SessionQuery {
            only: None,
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("读取会话列表");
    assert_eq!(summaries.len(), 1);
    assert_eq!(
        summaries[0].workspace(),
        None,
        "别名是 NULL 就是未分组：不得按 workspace_cwd 反查别名解析表补出归属"
    );
    store.close().await;

    // 前提复核：这个库确实「路径可匹配、别名匹配不上」，否则本用例没有判别力。
    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_session s JOIN owned_workspace w \
             ON w.canonical_path = s.workspace_cwd WHERE s.workspace_alias IS NULL",
        )
        .await,
        1,
        "前提：这条会话行的路径确实能在 owned_workspace 里按路径匹配到一行（否则反查断言恒真）"
    );
    pool.close().await;
}

// ---------------------------------------------------------------------------------------------
// 展示名解析与归属稳定
// ---------------------------------------------------------------------------------------------

/// 展示名解析的三种边界：已登记 → 取 `display_name`；不再登记 → 回退为**别名本身**；
/// 同一别名重指向别的目录 → 既有会话的**归属不变**（只有展示名跟着目录改名/删除变化）。
#[tokio::test]
async fn the_display_name_falls_back_to_the_alias_and_survives_a_re_pointed_alias() {
    let dir = temp_dir("tp4-display-name");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("新建库");
    let path = dir.join(DATABASE_FILE);
    store.close().await;

    let pool = raw_write_pool(&path).await;
    register_workspace(&pool, ALIAS, "Acp Remote", "probe").await;
    register_workspace(&pool, ALIAS_OTHER, "Office Main", "office").await;
    seed_session(&pool, SESSION, Some(&absolute_path("probe"))).await;
    // 直接写别名（这条用例测的是**读取投影**，不测写入路径）。
    sqlx::query("UPDATE owned_session SET workspace_alias = ?1 WHERE session_id = ?2")
        .bind(ALIAS)
        .bind(SESSION)
        .execute(&pool)
        .await
        .expect("给会话行带上别名");
    pool.close().await;

    // ① 别名已登记 → 展示名取 `owned_workspace.display_name`（不是别名本身，也不是路径）。
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    let workspace = store
        .list(SessionQuery {
            only: None,
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("读取会话列表")[0]
        .workspace()
        .expect("已分组的会话")
        .clone();
    assert_eq!(workspace.alias().as_str(), ALIAS);
    assert_eq!(
        workspace.display_name(),
        "Acp Remote",
        "别名已登记时展示名取目录记录"
    );
    store.close().await;

    // ② 同一别名被**重指向**另一个目录：既有会话的归属仍是该别名，`workspace_cwd` 保持创建时原值。
    let pool = raw_write_pool(&path).await;
    sqlx::query(
        "UPDATE owned_workspace SET canonical_path = ?2, display_name = ?3 WHERE alias = ?1",
    )
    .bind(ALIAS)
    .bind(absolute_path("relocated"))
    .bind("Acp Remote (moved)")
    .execute(&pool)
    .await
    .expect("把别名重指向另一个目录");
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    let summaries = store
        .list(SessionQuery {
            only: None,
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("读取会话列表");
    let workspace = summaries[0].workspace().expect("归属仍在").clone();
    assert_eq!(
        workspace.alias().as_str(),
        ALIAS,
        "别名重指向不改变既有会话的归属"
    );
    assert_eq!(
        workspace.display_name(),
        "Acp Remote (moved)",
        "目录改名会传播到投影取值"
    );
    store.close().await;

    let pool = raw_write_pool(&path).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT workspace_cwd FROM owned_session WHERE session_id = ?1",
        )
        .bind(SESSION)
        .fetch_one(&pool)
        .await
        .expect("读 workspace_cwd"),
        absolute_path("probe"),
        "重指向不得改写会话行创建时的规范化路径"
    );
    // ③ 目录被删除（别名不再登记）：LEFT JOIN 接不上，回退为别名本身——分组不消失，也不泄漏路径。
    sqlx::query("DELETE FROM owned_workspace WHERE alias = ?1")
        .bind(ALIAS)
        .execute(&pool)
        .await
        .expect("删除目录记录");
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    let workspace = store
        .list(SessionQuery {
            only: None,
            states: Vec::new(),
            limit: None,
        })
        .await
        .expect("读取会话列表")[0]
        .workspace()
        .expect("目录被删除也不丢分组")
        .clone();
    assert_eq!(workspace.alias().as_str(), ALIAS);
    assert_eq!(
        workspace.display_name(),
        ALIAS,
        "别名不再登记时展示名回退为别名本身（不是空串、不是路径）"
    );
    store.close().await;
}

// ---------------------------------------------------------------------------------------------
// 恢复不改写目录归属
// ---------------------------------------------------------------------------------------------

/// 恢复流程（窄读取 + `session.resume` 的 `accepted` 幂等提交，两者都不带 `workspace_alias`）
/// 前后，该列的字节相同。
#[tokio::test]
async fn the_resume_flow_neither_reads_nor_rewrites_the_alias() {
    let dir = temp_dir("tp4-resume-keeps-alias");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("新建库");
    let path = dir.join(DATABASE_FILE);
    let session = store
        .commit(create_session("resumable"))
        .await
        .expect("创建会话")
        .session_id
        .expect("分配的 sessionId");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
                agent_session_id: Some(AgentSessionId::new("acp-session-1").expect("会话标识")),
                workspace_cwd: Some(absolute_path("probe")),
                workspace_alias: Some(ALIAS.to_owned()),
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
        .expect("写入恢复数据与目录归属");
    store.close().await;

    let pool = raw_pool(&path).await;
    let before = quoted_alias(&pool, session.as_str()).await;
    assert_eq!(before, format!("'{ALIAS}'"));
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    // 恢复前的窄读取（`load_recovery`）**不读**该列：恢复数据里没有归属这一项。
    let record = store
        .load_recovery(&session)
        .await
        .expect("窄读取")
        .expect("两列都已写入");
    assert_eq!(
        record.workspace_cwd.as_deref(),
        Some(absolute_path("probe").as_str()),
        "恢复一律以持久化的 cwd 原文为权威"
    );
    // 恢复流程的存储交互：一条不带 `workspace_alias` 的提交（`state: None` + 幂等行）。
    let resumed = store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(),
            expected_version: None,
            state: Some(StateChange::Update(SessionUpdate {
                state: None,
                mode: ModeChange::Unchanged,
                closed_at: None,
                interaction: None,
                agent_session_id: None,
                workspace_cwd: None,
                workspace_alias: None,
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
        .expect("恢复流程的窄写");
    assert_eq!(
        resumed.version.get(),
        3,
        "恢复不改会话版本之外的东西：它既不写别名，也不改已持久化的恢复数据"
    );
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        quoted_alias(&pool, session.as_str()).await,
        before,
        "恢复前后该列的字节必须相同（`None` = 不改该列）"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT quote(workspace_cwd) FROM owned_session WHERE session_id = ?1",
        )
        .bind(session.as_str())
        .fetch_one(&pool)
        .await
        .expect("读 workspace_cwd"),
        format!("'{}'", absolute_path("probe")),
        "恢复不得以任何解析结果覆盖已持久化的 cwd"
    );
    pool.close().await;
}
