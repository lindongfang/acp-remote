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
//! - **强制输入（主 Agent 裁决转来）**：`ALTER 追加` 与 `12-step 重建` 的**判别式**断言；
//! - R21 的 `NULL` 读回语义（「不可恢复」而不是「行不存在」或「数据损坏」）；
//! - R13 的「过新拒绝」在**不写任何行**这一维度上的证据；
//! - R17 的「写入后可读回且不推导」在**原始字节**维度上的证据。

mod support;

use acp_core::model::{AgentSessionId, SessionId, Timestamp};
use acp_core::ports::SessionStore as _;
use storage_sqlite::migrate::{
    DATABASE_FILE, FILE_FORMAT_VERSION, OWNED_SCHEMA_VERSION, StorageConfig,
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

/// 把当前版本库回退成 **v4 形状**（去掉 v5/v6 才有的三列、降版本号），返回库文件路径。
///
/// 与既有 `migration.rs` 的 v4 构造手法同源（`DROP COLUMN` + `PRAGMA user_version` + `meta` 回写 +
/// `wal_checkpoint(TRUNCATE)`），**不新增 `fixtures/storage/v4/` 夹具**，避免与 WP4 的夹具族所有权纠缠。
/// v6 追加的 `workspace_directory_id`（目录归属）也必须一起去掉——否则 v5 段加完两列、v6 段再加一列时，
/// `ADD COLUMN` 会撞上「duplicate column」。本文件的判据仍只看**恢复两列**。
async fn rewind_to_v4(dir: &std::path::Path) -> std::path::PathBuf {
    let store = SqliteStore::open(StorageConfig::new(dir), &at())
        .await
        .expect("先建一个当前版本库再回退");
    store.close().await;
    let path = dir.join(DATABASE_FILE);
    let pool = raw_write_pool(&path).await;
    for column in [
        "agent_session_id",
        "workspace_cwd",
        "workspace_directory_id",
        "custom_title",
    ] {
        sqlx::query(&format!("ALTER TABLE owned_session DROP COLUMN {column}"))
            .execute(&pool)
            .await
            .expect("回退 owned_session 的恢复列");
    }
    sqlx::query("ALTER TABLE owned_device ADD COLUMN scopes_json TEXT NOT NULL DEFAULT '[]'")
        .execute(&pool)
        .await
        .expect("把 owned_device 补回 v8 之前就有的 scopes_json（v8 段只删这一列）");
    // v13 删除了 `closed_at`；当前库没有该列，合成的 v4 形状必须把它补回。
    sqlx::query("ALTER TABLE owned_session ADD COLUMN closed_at TEXT")
        .execute(&pool)
        .await
        .expect("把 owned_session 补回 v13 之前的 closed_at");
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
         VALUES (?1, ?2, 'fixture-agent', 'Fixture Agent', 'idle', ?3,
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
// v13：会话表重建为三态并删除 closed_at，恢复列保持可用。
// ---------------------------------------------------------------------------------------------

/// v4 → 当前形状：`owned_session` 被 v13 重建（去掉 `closed_at`、CHECK 收敛为三态），
/// 恢复三列保留在列清单末尾，既有行与设备行逐列保留。
///
/// 本用例在 `migration.rs` 的列清单相等断言之外补上**结构判别式**：
///
/// 1. 升级后的存储文本以 `CREATE TABLE owned_session (` 开头（v13 直接创建规范表，不靠 RENAME
///    临时表），且不含 `closed_at`、CHECK 只有三态；
/// 2. 列清单 =「v4 的 12 列去掉 closed_at」+ 恢复三列，`cid` 连续无空洞；
/// 3. 反证：v1 升级里确实重建的 `owned_audit` 文本带双引号表名（RENAME 的签名），说明「重建会改
///    存储文本」的判断有实际判别力。
#[tokio::test]
async fn v13_rebuilds_owned_session_without_closed_at_and_keeps_recovery_columns() {
    let dir = temp_dir("tp2-append-not-rebuild");
    let path = rewind_to_v4(&dir).await;
    let before_pool = raw_pool(&path).await;
    let ddl_before = ddl(&before_pool, "owned_session").await;
    let columns_before = column_specs(&before_pool, "owned_session").await;
    assert!(
        !ddl_before.contains("agent_session_id") && !ddl_before.contains("workspace_cwd"),
        "回退出来的库必须真的没有两列"
    );
    assert_eq!(
        columns_before.len(),
        12,
        "v4 的 owned_session 是 12 列（含 v13 删除的 closed_at）"
    );
    assert!(
        columns_before.iter().any(|(name, ..)| name == "closed_at"),
        "合成的 v4 形状必须带 closed_at"
    );
    before_pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("v4 库必须能升到当前三态形状");
    store.close().await;

    let pool = raw_pool(&path).await;
    let ddl_after = ddl(&pool, "owned_session").await;
    let columns_after = column_specs(&pool, "owned_session").await;

    // ① v13 重建会话表：`DROP` 旧表后把新表改名为规范名，SQLite 因此把存储文本记成带双引号的
    //    `CREATE TABLE "owned_session" (`；临时表在成功后已删除。保留列的名称/顺序/类型必须仍是
    //    v4 的次序，只去掉 `closed_at`。
    assert!(
        ddl_after.starts_with("CREATE TABLE \"owned_session\" ("),
        "v13 通过 RENAME 落成规范表名（表名带双引号）：{ddl_after}"
    );
    assert!(
        !ddl_after.contains("closed_at"),
        "v13 之后会话表不得再有 closed_at：{ddl_after}"
    );
    assert!(
        ddl_after.contains("CHECK (state IN ('idle','running','pending'))"),
        "会话状态 CHECK 必须收敛为三态：{ddl_after}"
    );
    for table in [
        "owned_session_v2",
        "owned_session_v3",
        "owned_session_v5",
        "owned_session_v6",
        "owned_session_v13_old",
        "owned_session_v13_new",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("查 sqlite_master"),
            0,
            "重建成功后不得留下临时表 {table}"
        );
    }

    // ② 列清单 =「v4 的 12 列去掉 closed_at」（前 11 列）+ 恢复三列 + v15 的 custom_title；
    //    `cid` 连续无空洞。
    assert_eq!(
        columns_after.len(),
        15,
        "当前 owned_session 是 15 列（v4 的 12 列去掉 closed_at + 恢复三列 + custom_title）"
    );
    assert_eq!(
        &columns_after[..11],
        &columns_before[..11],
        "保留列的名称/顺序/类型/NOT NULL/默认值必须逐项不变"
    );
    assert_eq!(columns_after[11].0, "agent_session_id");
    assert_eq!(columns_after[12].0, "workspace_cwd");
    assert_eq!(columns_after[13].0, "workspace_directory_id");
    assert_eq!(columns_after[14].0, "custom_title");
    assert_eq!(
        (
            &columns_after[11].1,
            &columns_after[12].1,
            &columns_after[13].1,
            &columns_after[14].1
        ),
        (
            &"TEXT".to_owned(),
            &"TEXT".to_owned(),
            &"TEXT".to_owned(),
            &"TEXT".to_owned()
        ),
        "四列都是 TEXT"
    );
    let cids: Vec<i64> = sqlx::query_scalar("SELECT cid FROM pragma_table_info('owned_session')")
        .fetch_all(&pool)
        .await
        .expect("读 cid");
    assert_eq!(cids, (0..15).collect::<Vec<i64>>(), "cid 必须连续无空洞");

    // ③ 反证：v1 升级路径里确实重建 `owned_audit`，其文本的表名**带双引号**（RENAME 的签名）——
    //    说明本文件的「重建会改存储文本」判断有实际判别力。
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
             agent_session_id, workspace_cwd)
             VALUES (?1, 't', 'fixture-agent', 'Fixture Agent', 'idle', ?2, NULL, NULL, 1, ?3, ?3,
             ?4, ?5)"#,
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
             agent_session_id, workspace_cwd)
             VALUES (?1, 't', 'fixture-agent', 'Fixture Agent', 'idle', ?2, NULL, NULL, 1, ?3, ?3,
             ?4, ?5)"#,
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
        store.close().await;
    }

    let pool = raw_pool(&path).await;
    let schema = schema_sql(&pool).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    assert_eq!(
        meta_version_of(&pool, "owned_schema_version").await,
        OWNED_SCHEMA_VERSION.to_string()
    );
    // 每条 owned 表的 DDL 都必须带上追加的三列（且只有 owned_session 带）。
    assert!(
        schema.iter().any(|(kind, name, sql)| kind == "table"
            && name == "owned_session"
            && sql.contains("agent_session_id")
            && sql.contains("workspace_cwd")
            && sql.contains("workspace_directory_id")),
        "owned_session 的 DDL 必须含追加的三列：{schema:?}"
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
