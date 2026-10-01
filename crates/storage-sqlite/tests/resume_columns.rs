//! R13–R21：`storage-sqlite` 的 v5 恢复列与读写边界（`storage-schema-v2-migration` 增量）。
//!
//! 入口是**真实 SQLite 文件**与真实 `storage_sqlite::migrate` 升级路径，断言读的是库文件里的字节
//! （`sqlite_master` / `quote()` / `PRAGMA table_info` / 端口 `load_recovery`），不是内存模型。
//!
//! **复用声明（避免第二份实现掩盖原用例失效）**：既有 `crates/storage-sqlite/tests/migration.rs`
//! 已经在 v5 目标上覆盖 R13/R14（`current_version_database_is_untouched_by_two_consecutive_starts`、
//! `second_open_of_an_upgraded_database_rewrites_nothing`）、R15（`a_failed_upgrade_rolls_back_to_v1`）、
//! R16/R18（`v4_database_upgrades_to_v5_by_appending_the_recovery_columns_only`）、R19
//! （`v3_database_upgrades_to_v5_…`）、R20（`v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks`）
//! 与 R17/R21/R22（`recovery_columns_round_trip_and_are_not_rewritten_by_the_resume_flow`）。
//! 本文件**不复制**那些断言，只补它们未覆盖的维度：
//!
//! - **CR4-F3（主 Agent 裁决转来的强制输入）**：`ALTER 追加` 与 `12-step 重建` 的**判别式**断言；
//! - R21 的 `NULL` 读回语义（「不可恢复」而不是「行不存在」或「数据损坏」）；
//! - R13 的「过新拒绝」在**不写任何行**这一维度上的证据；
//! - R17 的「写入后可读回且不推导」在**原始字节**维度上的证据。

mod support;

use acp_core::model::{AgentSessionId, SessionId, Timestamp};
use acp_core::ports::SessionStore as _;
use storage_sqlite::migrate::{
    DATABASE_FILE, FILE_FORMAT_VERSION, IMPORTED_SCHEMA_VERSION, OWNED_SCHEMA_VERSION,
    StorageConfig,
};
use storage_sqlite::session_store::SqliteStore;
use support::*;

/// 本文件的固定时间戳（`storage-sqlite` 不读系统时钟）。
const AT: &str = "2026-09-18T00:00:00.000Z";
/// 回退用例里的会话标识（uuid 形状）。
const SESSION: &str = "11111111-1111-4111-8111-111111111111";
/// 回退用例里的另一个会话标识。
const SESSION_OTHER: &str = "11111111-1111-4111-8111-111111111112";
/// 回退用例里的 origin epoch。
const EPOCH: &str = "33333333-3333-4333-8333-333333333333";

fn at() -> Timestamp {
    Timestamp::new(AT).expect("规范时间戳")
}

/// 把当前版本库回退成 **v4 形状**（去掉两列、降版本号），返回库文件路径。
///
/// 与既有 `migration.rs` 的 v4 构造手法同源（`DROP COLUMN` + `PRAGMA user_version` + `meta` 回写 +
/// `wal_checkpoint(TRUNCATE)`），**不新增 `fixtures/storage/v4/` 夹具**，避免与 WP4 的夹具族所有权纠缠。
async fn rewind_to_v4(dir: &std::path::Path) -> std::path::PathBuf {
    let store = SqliteStore::open(StorageConfig::new(dir), &at())
        .await
        .expect("先建一个当前版本库再回退");
    store.close().await;
    let path = dir.join(DATABASE_FILE);
    let pool = raw_write_pool(&path).await;
    for column in ["agent_session_id", "workspace_cwd"] {
        sqlx::query(&format!("ALTER TABLE owned_session DROP COLUMN {column}"))
            .execute(&pool)
            .await
            .expect("回退 owned_session 的恢复列");
    }
    sqlx::query("PRAGMA user_version = 4")
        .execute(&pool)
        .await
        .expect("回退文件格式版本");
    sqlx::query("UPDATE meta SET value = '4' WHERE key = 'owned_schema_version'")
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

/// 在一条 v4 形状的会话行里种下「除恢复两列外的全部列」（含 `NULL` 与非 `NULL` 两种取值）。
async fn seed_v4_session(path: &std::path::Path, session_id: &str, title: &str) {
    let pool = raw_write_pool(path).await;
    sqlx::query(
        r#"INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, origin_epoch,
         current_mode_id, current_mode_name, version, created_at, updated_at, closed_at)
         VALUES (?1, ?2, 'fixture-agent', 'Fixture Agent', 'waiting_input', ?3,
         'code', 'Code', 7, ?4, ?4, NULL)"#,
    )
    .bind(session_id)
    .bind(title)
    .bind(EPOCH)
    .bind(AT)
    .execute(&pool)
    .await
    .expect("种下 v4 形状的会话行");
    pool.close().await;
}

/// 一条会话行「除两列外」的全部列的 `quote()` 拼接（用于逐字节比对）。
async fn other_columns_quote(pool: &sqlx::SqlitePool, session_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        r#"SELECT 'session_id=' || quote(session_id)
           || '|title=' || quote(title)
           || '|agent_id=' || quote(agent_id)
           || '|agent_name=' || quote(agent_name)
           || '|state=' || quote(state)
           || '|origin_epoch=' || quote(origin_epoch)
           || '|current_mode_id=' || quote(current_mode_id)
           || '|current_mode_name=' || quote(current_mode_name)
           || '|version=' || quote(version)
           || '|created_at=' || quote(created_at)
           || '|updated_at=' || quote(updated_at)
           || '|closed_at=' || quote(closed_at)
           FROM owned_session WHERE session_id = ?1"#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .expect("读取会话行的其余列")
}

/// 一条表的 `sqlite_master` 原始 SQL 文本。
async fn ddl(pool: &sqlx::SqlitePool, table: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
    )
    .bind(table)
    .fetch_optional(pool)
    .await
    .expect("读 sqlite_master")
    .unwrap_or_else(|| panic!("表 {table} 必须存在于 sqlite_master"))
}

// ---------------------------------------------------------------------------------------------
// CR4-F3（强制输入）：v4→v5 必须是「ALTER 追加」，不得触发 12-step 重建。
// ---------------------------------------------------------------------------------------------

/// **判别式**断言：v4→v5 段对 `owned_session` 只做 `ALTER TABLE … ADD COLUMN`，不重建表。
///
/// 这正是 spec R13/R16 要求的性质（「两列只做追加，MUST NOT 触发 12-step 表重建」）。既有迁移用例
/// （`migration.rs`）对 `owned_session` 只断言 `after.contains("agent_session_id")`，因此**如果有人
/// 把 v5 段误写成重建**（例如为加 CHECK 而重建 `owned_session`），那些断言**仍然会通过**。本用例补上
/// 缺口，并且自带一条**反证**：本仓库的 12-step 重建段一律写成
/// `CREATE TABLE owned_x_vN (…) … DROP TABLE owned_x; ALTER TABLE owned_x_vN RENAME TO owned_x`，
/// SQLite 会把改名后的存储文本记成**带双引号的表名**（`CREATE TABLE "owned_x" (`），而
/// `ALTER TABLE … ADD COLUMN` 是**原地追加**、表名保持不带引号。本用例据此断言：
///
/// 1. `owned_session` 升级后的存储文本以**不带引号**的 `CREATE TABLE owned_session (` 开头（即追加
///    路径），且**不含**重建用的临时表名（`owned_session_v2` / `_v3` / `_v5`）。
///    **注意**：`IF NOT EXISTS` **不能**用作判别式——实测 `ALTER TABLE ADD COLUMN` 会把它从存储文本里
///    去掉，而重建文本本来就没有它，**两条路径都不含**，按它断言会恒真（实测表见
///    `reports/tp2-test-design.md §2.1`；本用例内联注释 ① 同样记录了这次标定）。判别式只认
///    「表名是否带双引号」。
/// 2. 既有列的**原始定义文本**（含列名后的空白与 `STRICT` 结尾）在升级后逐字节保留——重建会重排空白、
///    重新排版并改写 `STRICT` 之外的形式；
/// 3. 两列位于列清单**末尾**且是纯追加（`PRAGMA table_info` 的 `cid` 连续、无空洞）；
/// 4. 反证：v1 库升级里**确实会重建**的 `owned_audit`，其文本以**带双引号**的
///    `CREATE TABLE "owned_audit" (` 开头（RENAME 的签名）——证明本用例的判别式不是恒真。若把 ① 的
///    判别式方向写反或改成恒真表达式，本断言会立刻失败。
#[tokio::test]
async fn v5_appends_the_recovery_columns_instead_of_rebuilding_owned_session() {
    let dir = temp_dir("tp2-append-not-rebuild");
    let path = rewind_to_v4(&dir).await;
    let before_pool = raw_pool(&path).await;
    let ddl_before = ddl(&before_pool, "owned_session").await;
    let columns_before = column_specs(&before_pool, "owned_session").await;
    assert!(
        !ddl_before.contains("agent_session_id") && !ddl_before.contains("workspace_cwd"),
        "回退出来的库必须真的没有两列"
    );
    assert_eq!(columns_before.len(), 12, "v4 的 owned_session 是 12 列");
    before_pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("v4 库必须能升到 v5");
    store.close().await;

    let pool = raw_pool(&path).await;
    let ddl_after = ddl(&pool, "owned_session").await;
    let columns_after = column_specs(&pool, "owned_session").await;

    // ① **判别式（实测标定）**：本仓库的 12-step 重建段一律写成
    //    `CREATE TABLE owned_x_vN (...) … DROP TABLE owned_x; ALTER TABLE owned_x_vN RENAME TO owned_x`，
    //    SQLite 会把改名后的存储文本记成**带双引号的表名** `CREATE TABLE "owned_x"`；而 `ALTER TABLE …
    //    ADD COLUMN` 是**原地追加**，存储文本里表名保持不带引号。因此：
    //    - 追加路径 ⇒ `CREATE TABLE owned_session (`（无引号）；
    //    - 重建路径 ⇒ `CREATE TABLE "owned_session" (`（有引号）。
    //    （注意：`IF NOT EXISTS` **不能**用作判别式——实测 `ALTER TABLE ADD COLUMN` 会把它从存储文本里
    //    去掉，而重建文本本来就没有它，两条路径都不含，断言会恒真。此处已按实测标定。）
    assert!(
        ddl_after.starts_with("CREATE TABLE owned_session ("),
        "追加路径的存储文本必须是不带引号的表名；重建路径会写成带引号的 \"owned_session\"：{ddl_after}"
    );
    assert!(
        !ddl_after.starts_with("CREATE TABLE \"owned_session\" ("),
        "owned_session 被 12-step 重建了（表名带引号是 RENAME 的签名）：{ddl_after}"
    );
    // 反向：重建不得在本库留下任何临时表（追加不建任何对象）。
    for table in ["owned_session_v2", "owned_session_v3", "owned_session_v5"] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("查 sqlite_master"),
            0,
            "重建会先建临时表 {table} 再改名，追加不会"
        );
    }

    // ② 既有列的定义文本逐字节保留：`ALTER ADD COLUMN` 只在原文本末尾**拼接**追加列；
    //    重建会按重建脚本重新排版整段 DDL。取「原文本去掉表尾的 `)` 与其后的 `STRICT`」作为既有列
    //    部分（`DROP COLUMN` 会把表尾重排到同一行，因此这里按**最后一个**右括号切，不依赖换行）。
    let head_before = ddl_before
        .rfind(')')
        .map(|cut| ddl_before[..cut].trim_end().trim_end_matches(','))
        .unwrap_or_else(|| panic!("v4 文本形状异常（找不到表尾右括号）：{ddl_before}"));
    assert!(
        ddl_after.contains(head_before),
        "既有列的定义文本必须逐字节保留（重建会重排空白与排版）：\n{head_before}\n{ddl_after}"
    );
    // 追加列紧跟在最后一个既有列之后（SQLite 的追加渲染形状）。
    assert!(
        ddl_after.contains(&format!(
            "{head_before}, agent_session_id TEXT, workspace_cwd TEXT)"
        )),
        "两列必须紧跟既有列追加：{ddl_after}"
    );

    // ③ 列清单是「原 12 列 + 末尾两列」的严格追加，`cid` 连续无空洞。
    assert_eq!(columns_after.len(), 14, "v5 的 owned_session 是 14 列");
    assert_eq!(
        &columns_after[..12],
        &columns_before[..],
        "既有 12 列的名称/顺序/类型/NOT NULL/默认值必须逐项不变"
    );
    assert_eq!(columns_after[12].0, "agent_session_id");
    assert_eq!(columns_after[13].0, "workspace_cwd");
    assert_eq!(
        (&columns_after[12].1, &columns_after[13].1),
        (&"TEXT".to_owned(), &"TEXT".to_owned()),
        "两列都是 TEXT"
    );
    let cids: Vec<i64> = sqlx::query_scalar("SELECT cid FROM pragma_table_info('owned_session')")
        .fetch_all(&pool)
        .await
        .expect("读 cid");
    assert_eq!(
        cids,
        (0..14).collect::<Vec<i64>>(),
        "cid 必须连续无空洞（追加不会留空洞；重建会按重建脚本重排）"
    );

    // ④ 反证：v1→v5 的路径里**确实**重建 `owned_audit`，其文本的表名**带双引号**（RENAME 的签名）——
    //    因此 ① 的判别式不是恒真（若方向写反，本断言会立刻失败）。
    let v1_dir = temp_dir("tp2-append-not-rebuild-v1");
    let v1_path = copy_fixture("from-v1.sqlite3", &v1_dir);
    let v1_store = SqliteStore::open(StorageConfig::new(&v1_dir), &at())
        .await
        .expect("v1 库升级");
    v1_store.close().await;
    let v1_pool = raw_pool(&v1_path).await;
    let rebuilt = ddl(&v1_pool, "owned_audit").await;
    assert!(
        rebuilt.starts_with("CREATE TABLE \"owned_audit\" ("),
        "反证：确实走重建的表必须带引号表名，否则 ① 的判别式恒真：{rebuilt}"
    );
    assert!(
        !rebuilt.starts_with("CREATE TABLE owned_audit ("),
        "反证：重建文本不应是不带引号的表名：{rebuilt}"
    );
    v1_pool.close().await;
    pool.close().await;
}

// ---------------------------------------------------------------------------------------------
// R16 / R21：升级保留既有行，两列是 `NULL`；`NULL` 的读回语义是「没有恢复数据」。
// ---------------------------------------------------------------------------------------------

/// R16 + R21：v4→v5 之后既有会话行的其余列逐字节不变，两列是真正的 `NULL`（不是空串），且
/// `load_recovery` 读回 `None`——**不是** `NotFound`，也**不是**列值损坏错误。
#[tokio::test]
async fn upgraded_sessions_keep_their_bytes_and_report_no_recovery_data() {
    let dir = temp_dir("tp2-upgraded-null-columns");
    let path = rewind_to_v4(&dir).await;
    seed_v4_session(&path, SESSION, "kept session").await;
    seed_v4_session(&path, SESSION_OTHER, "second session").await;

    let before = raw_pool(&path).await;
    let kept_before = other_columns_quote(&before, SESSION).await.expect("会话行");
    let other_before = other_columns_quote(&before, SESSION_OTHER)
        .await
        .expect("会话行");
    before.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("升级到 v5");
    assert_eq!(store.metadata().owned_schema_version, OWNED_SCHEMA_VERSION);
    assert_eq!(
        store.metadata().imported_schema_version,
        IMPORTED_SCHEMA_VERSION
    );
    let session = SessionId::new(SESSION).expect("session id");
    // 端口层的读回语义：`NULL` 是「没有恢复数据」，因此是 `Ok(None)`（不是 Err，也不是别的形状）。
    assert_eq!(
        store
            .load_recovery(&session)
            .await
            .expect("读取升级后会话的恢复数据"),
        None,
        "升级后的既有会话不可恢复（两列 NULL）"
    );
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    assert_eq!(
        other_columns_quote(&pool, SESSION).await,
        Some(kept_before),
        "既有会话行除两列外必须逐字节不变"
    );
    assert_eq!(
        other_columns_quote(&pool, SESSION_OTHER).await,
        Some(other_before),
        "第二条既有会话行同样逐字节不变"
    );
    // 排除「空串 / 占位值」：两列必须是 SQL `NULL`（`typeof(...) = 'null'`），而不是 `''`。
    let nulls = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM owned_session \
         WHERE typeof(agent_session_id) = 'null' AND typeof(workspace_cwd) = 'null'",
    )
    .fetch_one(&pool)
    .await
    .expect("统计 NULL 列");
    assert_eq!(nulls, 2, "两行会话的两列都必须是 NULL");
    let placeholders = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM owned_session \
         WHERE agent_session_id = '' OR workspace_cwd = ''",
    )
    .fetch_one(&pool)
    .await
    .expect("统计空串");
    assert_eq!(placeholders, 0, "两列不得被空串或占位路径填上");
    pool.close().await;
}

/// R21：`NULL` 的读回语义在**只有一列**为 `NULL` 时同样成立（半条恢复数据不算恢复数据）。
///
/// 判别力：若实现改成「有一列就返回半条记录」，本用例失败。
#[tokio::test]
async fn a_half_null_recovery_pair_is_still_no_recovery_data() {
    let dir = temp_dir("tp2-half-null");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("新建库");
    let path = dir.join(DATABASE_FILE);
    store.close().await;

    let pool = raw_write_pool(&path).await;
    for (session_id, agent_session_id, workspace_cwd) in [
        (SESSION, Some("acp-1"), None),
        (SESSION_OTHER, None, Some("/work/only-cwd")),
    ] {
        sqlx::query(
            r#"INSERT INTO owned_session (session_id, title, agent_id, agent_name, state,
             origin_epoch, current_mode_id, current_mode_name, version, created_at, updated_at,
             closed_at, agent_session_id, workspace_cwd)
             VALUES (?1, 't', 'fixture-agent', 'Fixture Agent', 'idle', ?2, NULL, NULL, 1, ?3, ?3,
             NULL, ?4, ?5)"#,
        )
        .bind(session_id)
        .bind(EPOCH)
        .bind(AT)
        .bind(agent_session_id)
        .bind(workspace_cwd)
        .execute(&pool)
        .await
        .expect("种下半条恢复数据");
    }
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开");
    for session_id in [SESSION, SESSION_OTHER] {
        let session = SessionId::new(session_id).expect("session id");
        assert_eq!(
            store.load_recovery(&session).await.expect("读回"),
            None,
            "{session_id}: 缺任一列都不是可用的恢复数据（不得返回半条记录）"
        );
    }
    // 行不存在与两列为 NULL 都不产出恢复记录，但前者是「没有这行」，后者是「这行不可恢复」——
    // 两者的区别由存储层的行存在性独立证明（本断言只保证都不返回数据）。
    let absent = SessionId::new("99999999-9999-4999-8999-999999999999").expect("session id");
    assert_eq!(store.load_recovery(&absent).await.expect("读回"), None);
    store.close().await;
}

/// R17：新列写入后**按字节**读回（不经任何归一化），未写入的行读回 `None`。
#[tokio::test]
async fn recovery_columns_round_trip_byte_exactly_and_are_never_derived() {
    let dir = temp_dir("tp2-round-trip");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("新建库");
    let path = dir.join(DATABASE_FILE);

    let pool = raw_write_pool(&path).await;
    // 直接写两列（含一个**非规范化形状**的 cwd，用来证明读回不做归一化）。
    let raw_cwd = "/work/demo/./nested/";
    for (session_id, agent_session_id, workspace_cwd) in [
        (SESSION, Some("acp-session-1"), Some(raw_cwd)),
        // 第二行两列都是 NULL：读回必须仍是「无恢复数据」，而不是从第一行推导。
        (SESSION_OTHER, None, None),
    ] {
        sqlx::query(
            r#"INSERT INTO owned_session (session_id, title, agent_id, agent_name, state,
             origin_epoch, current_mode_id, current_mode_name, version, created_at, updated_at,
             closed_at, agent_session_id, workspace_cwd)
             VALUES (?1, 't', 'fixture-agent', 'Fixture Agent', 'idle', ?2, NULL, NULL, 1, ?3, ?3,
             NULL, ?4, ?5)"#,
        )
        .bind(session_id)
        .bind(EPOCH)
        .bind(AT)
        .bind(agent_session_id)
        .bind(workspace_cwd)
        .execute(&pool)
        .await
        .expect("种下会话行");
    }
    pool.close().await;
    store.close().await;

    // 重开一个**新的 store 实例**：读回必须来自库文件，而不是进程内缓存。
    let reopened = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("重开存储");
    let session = SessionId::new(SESSION).expect("session id");
    let record = reopened
        .load_recovery(&session)
        .await
        .expect("读回")
        .expect("写入过的两列必须读回");
    assert_eq!(
        record.agent_session_id,
        Some(AgentSessionId::new("acp-session-1").expect("会话标识")),
        "ACP 会话标识按写入字节读回"
    );
    assert_eq!(
        record.workspace_cwd.as_deref(),
        Some(raw_cwd),
        "workspace_cwd 按写入的**原文**读回（存储层不做规范化，也不解析别名）"
    );
    let other = SessionId::new(SESSION_OTHER).expect("session id");
    assert_eq!(
        reopened.load_recovery(&other).await.expect("读回"),
        None,
        "没有写入两列的行不得出现推导值（空串、别名或占位路径）"
    );
    reopened.close().await;

    // 库文件字节层面的复核：写入的取值没有被存储层改写。
    let pool = raw_pool(&path).await;
    let stored = sqlx::query_scalar::<_, String>(
        "SELECT quote(agent_session_id) || '|' || quote(workspace_cwd) FROM owned_session \
         WHERE session_id = ?1",
    )
    .bind(SESSION)
    .fetch_one(&pool)
    .await
    .expect("读库内原始字节");
    assert_eq!(stored, "'acp-session-1'|'/work/demo/./nested/'");
    pool.close().await;
}

/// R13：过新版本被拒绝打开，且**一个字节都不写**。
///
/// 判别力：若实现在拒绝前先写了点什么（例如补 `meta` 键或推进 `user_version`），行快照与 DDL 快照
/// 的相等断言会失败。
#[tokio::test]
async fn a_too_new_database_is_refused_without_touching_a_single_byte() {
    let dir = temp_dir("tp2-too-new");
    let path = dir.join(DATABASE_FILE);
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("先建一个当前版本库");
    store.close().await;

    let pool = raw_write_pool(&path).await;
    sqlx::query(&format!(
        "PRAGMA user_version = {}",
        FILE_FORMAT_VERSION + 1
    ))
    .execute(&pool)
    .await
    .expect("把版本顶到过新");
    pool.close().await;

    let before_pool = raw_pool(&path).await;
    let tables_before = table_snapshot(&before_pool, "owned_session").await;
    let schema_before = schema_sql(&before_pool).await;
    let meta_before = meta_rows(&before_pool).await;
    let version_before = scalar_i64(&before_pool, "PRAGMA user_version").await;
    before_pool.close().await;

    let refused = SqliteStore::open(StorageConfig::new(&dir), &at()).await;
    assert!(
        refused.is_err(),
        "过新版本必须被拒绝打开（当前版本 {FILE_FORMAT_VERSION}）"
    );

    let after_pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&after_pool, "PRAGMA user_version").await,
        version_before,
        "被拒绝打开不得推进版本"
    );
    assert_eq!(
        table_snapshot(&after_pool, "owned_session").await,
        tables_before,
        "被拒绝打开不得写任何行"
    );
    assert_eq!(
        schema_sql(&after_pool).await,
        schema_before,
        "被拒绝打开不得改任何 DDL"
    );
    assert_eq!(
        meta_rows(&after_pool).await,
        meta_before,
        "被拒绝打开不得补写 meta 键"
    );
    after_pool.close().await;
}

/// R13/R14：连续两次打开，版本常量与库内每条 DDL 文本逐字节相同（在此基线上独立复核既有断言）。
#[tokio::test]
async fn two_consecutive_opens_leave_the_schema_byte_identical() {
    let dir = temp_dir("tp2-two-opens");
    let path = rewind_to_v4(&dir).await;
    seed_v4_session(&path, SESSION, "twice").await;

    for round in 1..=2 {
        let store = SqliteStore::open(StorageConfig::new(&dir), &at())
            .await
            .unwrap_or_else(|error| panic!("第 {round} 次打开必须成功：{error}"));
        assert_eq!(store.metadata().owned_schema_version, OWNED_SCHEMA_VERSION);
        assert_eq!(
            store.metadata().imported_schema_version,
            IMPORTED_SCHEMA_VERSION
        );
        store.close().await;
    }

    let pool = raw_pool(&path).await;
    let schema = schema_sql(&pool).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    assert_eq!(meta_version_of(&pool, "owned_schema_version").await, "5");
    assert_eq!(meta_version_of(&pool, "imported_schema_version").await, "3");
    // 每条 owned 表的 DDL 都必须带上两列（且只有 owned_session 带）。
    assert!(
        schema.iter().any(|(kind, name, sql)| kind == "table"
            && name == "owned_session"
            && sql.contains("agent_session_id")
            && sql.contains("workspace_cwd")),
        "owned_session 的 DDL 必须含两列：{schema:?}"
    );
    // 再开第三次仍不重写任何文本。
    pool.close().await;
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("第三次打开");
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        schema_sql(&pool).await,
        schema,
        "重复打开不得改写任何 DDL 文本"
    );
    pool.close().await;
}

async fn meta_version_of(pool: &sqlx::SqlitePool, key: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT value FROM meta WHERE key = ?1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .expect("读 meta")
        .unwrap_or_else(|| panic!("meta.{key} 必须存在"))
}
