//! §9.1/§9.28：migration 的幂等、「版本过新拒绝启动且不写入任何行」与 **旧库升级的数据保留**。
//!
//! 夹具 `fixtures/storage/v2/` 三件套在 v3 之后是**冻结的历史升级输入**：`empty.sqlite3` 是 v2 形状的
//! 空库（v2 → v3 升级用例的输入），`from-v1.sqlite3` 是 v1 库（v1 → v2 → v3 连续升级的输入），
//! `too-new.sqlite3`（`user_version = 3`）在 v3 之后不再「过新」，过新用例改为用 `empty.sqlite3` 的临时
//! 副本判定（把该副本的 `user_version` 顶到 `FILE_FORMAT_VERSION + 1`）。`fixtures/storage/v1/` 的两个
//! 文件是更早的历史资产，只被下面的 `from-v1.sqlite3` 生成器当作基底读取。

mod support;

use acp_core::model::{AuditAction, EventId, ExportId, Sequence, Timestamp};
use storage_sqlite::error::StorageError;
use storage_sqlite::migrate::{FILE_FORMAT_VERSION, StorageConfig}; // probe
use storage_sqlite::session_store::SqliteStore;
use support::*;

/// v2 的全部表：`too-new` 用例用它做「拒绝启动时一行都不写」的逐表快照。
const TABLES: &[&str] = &[
    "meta",
    "owned_session",
    "owned_turn",
    "owned_event",
    "owned_command",
    "owned_interaction",
    "owned_audit",
    "owned_attachment",
    "owned_attachment_link",
    "owned_device",
    "owned_node",
    "owned_peer_key",
    "owned_pairing",
    "owned_pairing_peer",
    "owned_export",
    "owned_agent_profile",
    "owned_workspace",
    "owned_provider_ref",
    "imported_import",
    "imported_session",
    "imported_delivery_index",
    "imported_command_ref",
    "imported_audit",
    "imported_import_export",
];

fn at() -> Timestamp {
    Timestamp::new(AT).expect("timestamp")
}

fn still_writable(dir: &std::path::Path) -> StorageConfig {
    StorageConfig::new(dir)
}

async fn ints(pool: &sqlx::SqlitePool, sql: &str) -> Vec<i64> {
    sqlx::query_scalar(sql)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|error| panic!("{sql}: {error}"))
}

async fn texts(pool: &sqlx::SqlitePool, sql: &str) -> Vec<String> {
    sqlx::query_scalar(sql)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|error| panic!("{sql}: {error}"))
}

/// 取某张表在 `sqlite_master` 里的 DDL 文本（表不存在时返回空串）。
async fn table_ddl(pool: &sqlx::SqlitePool, table: &str) -> String {
    texts(
        pool,
        &format!("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '{table}'"),
    )
    .await
    .first()
    .cloned()
    .unwrap_or_default()
}

/// 往 `owned_audit` 插一行审计（§7.3 的列子集）；取值是否被 CHECK 接受由调用方断言。
async fn insert_audit_row(
    pool: &sqlx::SqlitePool,
    audit_id: i64,
    action: &str,
    actor_kind: &str,
    actor_id: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO owned_audit (audit_id, at, action, actor_kind, actor_id, target_kind, \
         target_id, outcome) VALUES (?1, ?2, ?3, ?4, ?5, 'export', ?6, 'success')",
    )
    .bind(audit_id)
    .bind(AT)
    .bind(action)
    .bind(actor_kind)
    .bind(actor_id)
    .bind(FIXTURE_EXPORT)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 往 `owned_command` 插一行最简查询命令：用于断言 `actor_kind` 的 CHECK 只接受三个值。
async fn insert_command_row(pool: &sqlx::SqlitePool, actor_kind: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO owned_command (actor_kind, actor_id, request_id, command, kind, \
         request_fingerprint, accepted_at, status) \
         VALUES (?1, 'cli', 'fixture-request', 'session.list', 'query', 'fp', ?2, 'accepted')",
    )
    .bind(actor_kind)
    .bind(AT)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// **已经是当前版本**的库连续两次启动后必须逐一字节不变（§9.1 的幂等判据）。
///
/// 起点是空目录新建出来的当前版本库：v2 之后的旧夹具不再代表「当前形状」，因此「已是最新版则不重写」
/// 改在真实新建的库上判定；升级后的库由 `second_open_of_an_upgraded_database_rewrites_nothing` 逐行断言。
#[tokio::test]
async fn current_version_database_is_untouched_by_two_consecutive_starts() {
    let dir = temp_dir("migrate-idempotent");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("create store");
    store.close().await;
    let path = dir.join("acp-remote.sqlite3");

    let pool = raw_pool(&path).await;
    let before_schema = schema_sql(&pool).await;
    let before_meta = meta_rows(&pool).await;
    let before_version = scalar_i64(&pool, "PRAGMA user_version").await;
    assert_eq!(before_version, FILE_FORMAT_VERSION);
    let server_epoch = before_meta
        .iter()
        .find(|(key, _)| key == "server_epoch")
        .map(|(_, value)| value.clone())
        .expect("a fresh database carries server_epoch");
    let mut before_rows = Vec::new();
    for table in TABLES {
        before_rows.push((*table, table_snapshot(&pool, table).await));
    }
    pool.close().await;

    for round in 1..=2 {
        let store = SqliteStore::open(still_writable(&dir), &at())
            .await
            .unwrap_or_else(|error| panic!("round {round} must open: {error}"));
        assert_eq!(store.metadata().owned_schema_version, 3);
        assert_eq!(store.metadata().imported_schema_version, 3);
        store.close().await;

        let pool = raw_pool(&path).await;
        assert_eq!(
            scalar_i64(&pool, "PRAGMA user_version").await,
            before_version,
            "round {round} changed user_version"
        );
        assert_eq!(
            schema_sql(&pool).await,
            before_schema,
            "round {round} rewrote sqlite_master"
        );
        assert_eq!(
            meta_rows(&pool).await,
            before_meta,
            "round {round} rewrote meta"
        );
        assert_eq!(
            meta_rows(&pool)
                .await
                .iter()
                .find(|(key, _)| key == "server_epoch")
                .map(|(_, value)| value.clone()),
            Some(server_epoch.clone()),
            "server_epoch must survive restarts unchanged"
        );
        for (table, before) in &before_rows {
            assert_eq!(
                table_snapshot(&pool, table).await,
                *before,
                "round {round} changed rows in {table}"
            );
        }
        pool.close().await;
    }
}

/// 新建库直接把 `user_version` 与两族 schema 版本写到当前版本。
#[tokio::test]
async fn fresh_directory_is_created_at_the_current_version() {
    let dir = temp_dir("migrate-fresh");
    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("create store");
    store.close().await;

    let pool = raw_pool(&dir.join("acp-remote.sqlite3")).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    let meta = meta_rows(&pool).await;
    for key in [
        "server_epoch",
        "created_at",
        "last_prune_at",
        "owned_schema_version",
        "imported_schema_version",
    ] {
        assert!(
            meta.iter().any(|(name, _)| name == key),
            "meta.{key} must exist"
        );
    }
    for key in ["owned_schema_version", "imported_schema_version"] {
        assert_eq!(
            meta.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str()),
            Some("3"),
            "meta.{key} must be written at the current version"
        );
    }
    assert_eq!(
        meta.iter()
            .find(|(key, _)| key == "created_at")
            .map(|(_, value)| value.as_str()),
        Some(AT)
    );
    pool.close().await;
}

/// §9.1/§9.28：`user_version` 高于本二进制已知版本的库必须被具名拒绝，且一行都不写。
///
/// v3 之后 `fixtures/storage/v2/too-new.sqlite3`（`user_version = 3`）不再「过新」，因此判据改在临时
/// 副本上判定：拿当前版本的历史夹具（v2 形状的空库）把版本顶到 `FILE_FORMAT_VERSION + 1`。
#[tokio::test]
async fn too_new_database_is_rejected_without_writing_rows() {
    let dir = temp_dir("migrate-too-new");
    let path = copy_fixture("empty.sqlite3", &dir);
    let future_version = FILE_FORMAT_VERSION + 1;
    let pool = raw_write_pool(&path).await;
    sqlx::query(&format!("PRAGMA user_version = {future_version}"))
        .execute(&pool)
        .await
        .expect("forge a newer file format");
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .expect("checkpoint");
    pool.close().await;

    let pool = raw_pool(&path).await;
    let before_schema = schema_sql(&pool).await;
    let before_meta = meta_rows(&pool).await;
    let mut before_rows = Vec::new();
    for table in TABLES {
        before_rows.push((*table, table_snapshot(&pool, table).await));
    }
    pool.close().await;

    let error = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect_err("a newer file format must be rejected");
    match error {
        StorageError::FileFormatTooNew { found, supported } => {
            assert_eq!(found, future_version);
            assert_eq!(supported, FILE_FORMAT_VERSION);
        }
        other => panic!("expected FileFormatTooNew, got {other}"),
    }

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        future_version
    );
    assert_eq!(schema_sql(&pool).await, before_schema);
    assert_eq!(meta_rows(&pool).await, before_meta);
    for (table, before) in before_rows {
        assert_eq!(
            table_snapshot(&pool, table).await,
            before,
            "{table} must be untouched after a rejected start"
        );
    }
    pool.close().await;
}

/// §9.28：v1 → v2 → v3 连续升级保留事件序号、origin cursor、幂等行与全部审计，`audit_id` 与其
/// AUTOINCREMENT 序列不回退，审计表的新取值可用；Import 的 Export 关联搬到关联表，**不补 grants**。
#[tokio::test]
async fn v1_fixture_upgrades_to_v3_and_preserves_rows() {
    let dir = temp_dir("migrate-from-v1");
    let path = copy_fixture("from-v1.sqlite3", &dir);

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        1,
        "the fixture must start life as a v1 database"
    );
    let server_epoch_before = meta_rows(&pool)
        .await
        .into_iter()
        .find(|(key, _)| key == "server_epoch")
        .map(|(_, value)| value)
        .expect("fixture carries server_epoch");
    let audit_ids_before = ints(&pool, "SELECT audit_id FROM owned_audit ORDER BY audit_id").await;
    assert_eq!(audit_ids_before, vec![1, 2, 5]);
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT seq FROM sqlite_sequence WHERE name = 'owned_audit'"
        )
        .await,
        7,
        "the fixture keeps an audit sequence that is ahead of max(audit_id)"
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT seq FROM sqlite_sequence WHERE name = 'imported_audit'"
        )
        .await,
        3,
        "the imported side is ahead of its max(audit_id) too"
    );
    pool.close().await;

    let store = SqliteStore::open(StorageConfig::new(&dir), &at())
        .await
        .expect("a v1 database must upgrade to the current version");
    assert_eq!(store.metadata().owned_schema_version, 3);
    assert_eq!(store.metadata().imported_schema_version, 3);

    // spec 的「升级后重放与幂等仍一致」：读视图必须给出升级前那三条事件，且正文能经
    // `event_payload` 还原——不是「行还在但读不出来」。
    // spec 的「升级后重放与幂等仍一致」：读视图必须给出升级前那三条事件，且正文能经
    // `event_payload` 还原——不是「行还在但读不出来」。读视图持有读池连接，必须在 `close()` 之前释放，
    // 否则 `close()` 会一直等这条连接归还。
    // `Box<dyn ReadView>` 上的方法不需要 trait 在作用域内；`read_view` 本身来自 `SessionStore`。
    use acp_core::ports::SessionStore as _;
    let view = store.read_view().await.expect("read view after upgrade");
    let head = view.head().await.expect("head");
    assert_eq!(
        head.global_sequence,
        Sequence::new(3).expect("sequence"),
        "升级不得让事件序号漂移"
    );
    for fixture_event in [FIXTURE_EVENT_ONE, FIXTURE_EVENT_TWO, FIXTURE_EVENT_THREE] {
        let id = EventId::new(fixture_event).expect("event id");
        let payload = view.event_payload(&id).await.expect("event payload");
        assert!(
            payload.is_some(),
            "升级后事件正文必须仍可读：{fixture_event}"
        );
    }
    drop(view);

    // 升级后的管理读路径也要可用（spec「升级后重放与幂等仍一致」的另一半）：`imported_import` 已
    // 拆分，Export 关联迁进 `imported_import_export`——经端口读回必须仍是同一对 owner/export。
    use acp_core::ports::ExportStore as _;
    let imports = store.imports().await.expect("imports after upgrade");
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].owner_endpoint(), "wss://owner.invalid");
    assert_eq!(
        imports[0].export_ids(),
        [ExportId::new(FIXTURE_EXPORT).expect("export id")].as_slice(),
        "Export 关联必须经 imported_import_export 原样读回"
    );
    store.close().await;

    let pool = raw_write_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    assert_eq!(
        meta_rows(&pool)
            .await
            .into_iter()
            .find(|(key, _)| key == "server_epoch")
            .map(|(_, value)| value),
        Some(server_epoch_before),
        "server_epoch must survive the upgrade"
    );
    for key in ["owned_schema_version", "imported_schema_version"] {
        assert_eq!(
            meta_rows(&pool)
                .await
                .into_iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            Some("3".to_owned()),
            "meta.{key} must be advanced to the current version"
        );
    }

    // 事件：序号、会话内序号与 origin cursor 都不重新编号。
    assert_eq!(
        ints(
            &pool,
            "SELECT global_sequence FROM owned_event ORDER BY global_sequence"
        )
        .await,
        vec![1, 2, 3]
    );
    assert_eq!(
        ints(
            &pool,
            "SELECT session_sequence FROM owned_event ORDER BY global_sequence"
        )
        .await,
        vec![1, 2, 3]
    );
    assert_eq!(
        ints(
            &pool,
            "SELECT origin_sequence FROM owned_event ORDER BY global_sequence"
        )
        .await,
        vec![1, 2, 3]
    );
    assert_eq!(
        texts(&pool, "SELECT DISTINCT origin_epoch FROM owned_event").await,
        vec![FIXTURE_EPOCH.to_owned()]
    );
    assert_eq!(
        texts(
            &pool,
            "SELECT event_id FROM owned_event ORDER BY global_sequence"
        )
        .await,
        vec![
            FIXTURE_EVENT_ONE.to_owned(),
            FIXTURE_EVENT_TWO.to_owned(),
            FIXTURE_EVENT_THREE.to_owned(),
        ]
    );

    // 幂等行：requestId、终态与终态事件引用都保留。
    assert_eq!(
        texts(
            &pool,
            "SELECT request_id || '|' || status || '|' || terminal_event_id FROM owned_command"
        )
        .await,
        vec![format!("{FIXTURE_REQUEST}|completed|{FIXTURE_EVENT_ONE}")]
    );

    // 审计：行与 audit_id 保留，序列不回退，新取值可用。
    assert_eq!(
        ints(&pool, "SELECT audit_id FROM owned_audit ORDER BY audit_id").await,
        audit_ids_before
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT seq FROM sqlite_sequence WHERE name = 'owned_audit'"
        )
        .await,
        7,
        "the rebuilt table must not rewind the AUTOINCREMENT sequence"
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT seq FROM sqlite_sequence WHERE name = 'imported_audit'"
        )
        .await,
        3,
        "the rebuilt imported table must not rewind the sequence either"
    );
    let audit_ddl: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'owned_audit'",
    )
    .fetch_one(&pool)
    .await
    .expect("owned_audit ddl");
    assert!(
        audit_ddl.contains("provider.configured"),
        "the 12-step rebuild must install the widened CHECK:\n{audit_ddl}"
    );
    sqlx::query(
        "INSERT INTO owned_audit (at, action, actor_kind, actor_id, target_kind, target_id, outcome) \
         VALUES (?1, 'provider.configured', 'cli', 'local', 'provider', 'openai', 'success')",
    )
    .bind(AT)
    .execute(&pool)
    .await
    .expect("a v2-only audit action must be writable after the upgrade");
    assert_eq!(
        scalar_i64(&pool, "SELECT MAX(audit_id) FROM owned_audit").await,
        8,
        "the new row must continue the preserved sequence"
    );

    // Import：管理行不再有 export_id、不补 grants；Export 关联搬到关联表（added_at 取原 created_at）。
    assert!(
        !column_names(&pool, "imported_import")
            .await
            .contains(&"export_id".to_owned()),
        "imported_import must not carry export_id any more"
    );
    assert_eq!(
        texts(&pool, "SELECT grants_json FROM imported_import").await,
        vec!["[]".to_owned()],
        "grants without a trustworthy source must not be filled in"
    );
    assert_eq!(
        texts(
            &pool,
            "SELECT import_id || '|' || owner_node_id || '|' || export_id || '|' || added_at \
             FROM imported_import_export"
        )
        .await,
        vec![format!(
            "{FIXTURE_IMPORT}|{FIXTURE_NODE}|{FIXTURE_EXPORT}|{}",
            at()
        )]
    );

    // cursor/ACK、交付索引与命令引用逐行保留。
    assert_eq!(
        ints(&pool, "SELECT last_origin_sequence FROM imported_session").await,
        vec![FIXTURE_LAST_ORIGIN_SEQUENCE]
    );
    assert_eq!(
        ints(&pool, "SELECT acked_origin_sequence FROM imported_session").await,
        vec![FIXTURE_ACKED_ORIGIN_SEQUENCE]
    );
    assert_eq!(
        ints(&pool, "SELECT local_sequence FROM imported_delivery_index").await,
        vec![FIXTURE_LOCAL_SEQUENCE]
    );
    assert_eq!(
        texts(&pool, "SELECT request_id FROM imported_command_ref").await,
        vec![FIXTURE_IMPORT_REQUEST.to_owned()]
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM imported_audit").await,
        2
    );

    // spec 的「黄金列清单逐项相等」：升级库的 `imported_*` 列必须与**新建库**逐项相等——
    // 否则重建脚本的列名/列集合可以悄悄与 DDL 常量分叉。
    let fresh_dir = temp_dir("migrate-fresh-columns");
    let fresh_store = SqliteStore::open(StorageConfig::new(&fresh_dir), &at())
        .await
        .expect("fresh v2 store");
    fresh_store.close().await;
    let fresh_pool = raw_pool(&fresh_dir.join("acp-remote.sqlite3")).await;
    for table in [
        "imported_import",
        "imported_import_export",
        "imported_session",
        "imported_delivery_index",
        "imported_command_ref",
        "imported_audit",
    ] {
        assert_eq!(
            column_names(&pool, table).await,
            column_names(&fresh_pool, table).await,
            "升级库的 {table} 列必须与新建库逐项相等"
        );
    }
    fresh_pool.close().await;

    // 升级库的两张审计表 DDL 必须列出**全部** `AuditAction` 取值：12-step 重建少写一个取值就会红
    // （新建库一侧由 `enum_coverage.rs` 的逐值断言覆盖）。
    for table in ["owned_audit", "imported_audit"] {
        let ddl = texts(
            &pool,
            &format!("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '{table}'"),
        )
        .await;
        let ddl = ddl.first().cloned().unwrap_or_default();
        for action in AuditAction::ALL {
            assert!(
                ddl.contains(action.as_str()),
                "{table} 的 CHECK 缺少 {}：{ddl}",
                action.as_str()
            );
        }
    }
    pool.close().await;
}

/// §7.2/§9.28：**v2 → v3** 只扩宽两张审计表的 `actor_kind`/`action` CHECK——行、`audit_id` 与
/// `AUTOINCREMENT` 序列都不动，`owned_command` 的 `actor_kind` 仍是三值（认领方永不提交命令）。
#[tokio::test]
async fn v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks() {
    let dir = temp_dir("migrate-from-v2");
    let path = copy_fixture("empty.sqlite3", &dir);

    // 夹具必须真的长着 v2 的样子：`user_version = 2`，且两张审计表的 CHECK 还不接受新取值。
    let pool = raw_write_pool(&path).await;
    assert_eq!(scalar_i64(&pool, "PRAGMA user_version").await, 2);
    for table in ["owned_audit", "imported_audit"] {
        let ddl = table_ddl(&pool, table).await;
        assert!(
            !ddl.contains("pairing_claimant"),
            "{table} 应是 v2 形状的 CHECK：{ddl}"
        );
        assert!(
            !ddl.contains("node.authenticated"),
            "{table} 应是 v2 形状的 CHECK：{ddl}"
        );
    }
    // 种入 v2 取值可写的审计行，并留下「序列领先于 max(audit_id)」的空洞（同 28 判据的判别力来源）。
    for (audit_id, action, actor_kind, actor_id) in [
        (1_i64, "device.authenticated", "device", FIXTURE_NODE),
        (2, "node.paired", "node", FIXTURE_NODE),
        (7, "export.created", "cli", "cli"),
    ] {
        insert_audit_row(&pool, audit_id, action, actor_kind, actor_id)
            .await
            .expect("v2 的审计取值必须可写");
    }
    sqlx::query("DELETE FROM owned_audit WHERE audit_id = 7")
        .execute(&pool)
        .await
        .expect("hole");
    sqlx::query(
        "INSERT INTO imported_audit (at, action, actor_kind, actor_id, target_kind, target_id, \
         outcome) VALUES (?1, 'node.paired', 'node', ?2, 'node', ?2, 'success')",
    )
    .bind(AT)
    .bind(FIXTURE_NODE)
    .execute(&pool)
    .await
    .expect("seed imported audit");
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .expect("checkpoint");
    // 升级前：v2 形状的 CHECK 必须真的拒绍新词表（否则下面的「升级后可写」不具判别力）。
    let refused_action =
        insert_audit_row(&pool, 8, "node.authenticated", "node", FIXTURE_NODE).await;
    assert!(
        refused_action.is_err(),
        "v2 形状的 CHECK 不应接受 node.authenticated：{refused_action:?}"
    );
    let refused_actor = insert_audit_row(
        &pool,
        8,
        "pairing.created",
        "pairing_claimant",
        FIXTURE_PAIRING,
    )
    .await;
    assert!(
        refused_actor.is_err(),
        "v2 形状的 CHECK 不应接受 pairing_claimant：{refused_actor:?}"
    );
    pool.close().await;

    let store = SqliteStore::open(still_writable(&dir), &at())
        .await
        .expect("a v2 database must upgrade to the current version");
    assert_eq!(store.metadata().owned_schema_version, 3);
    assert_eq!(store.metadata().imported_schema_version, 3);
    store.close().await;

    let pool = raw_write_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        FILE_FORMAT_VERSION
    );
    for key in ["owned_schema_version", "imported_schema_version"] {
        assert_eq!(
            meta_rows(&pool)
                .await
                .into_iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            Some("3".to_owned()),
            "meta.{key} 必须升到 3"
        );
    }
    // 行与 `audit_id` 逐行保留；序列不回退（7 > max(audit_id) = 2）。
    assert_eq!(
        ints(&pool, "SELECT audit_id FROM owned_audit ORDER BY audit_id").await,
        vec![1, 2]
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT seq FROM sqlite_sequence WHERE name = 'owned_audit'"
        )
        .await,
        7,
        "12-step 重建不得把 AUTOINCREMENT 序列压回 max(audit_id)"
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM imported_audit").await,
        1
    );
    // 新取值在升级库上可写：两个 node 动作 + 认领方的 `actor_kind`。
    insert_audit_row(&pool, 20, "node.authenticated", "node", FIXTURE_NODE)
        .await
        .expect("node.authenticated 必须可写");
    insert_audit_row(&pool, 21, "node.auth_failed", "node", FIXTURE_NODE)
        .await
        .expect("node.auth_failed 必须可写");
    insert_audit_row(
        &pool,
        22,
        "pairing.claimed",
        "pairing_claimant",
        FIXTURE_PAIRING,
    )
    .await
    .expect("pairing_claimant 必须可写");
    // `owned_command` 不动：认领方不会也不得提交命令。
    insert_command_row(&pool, "cli")
        .await
        .expect("旧取值仍可写");
    let refused = insert_command_row(&pool, "pairing_claimant").await;
    assert!(
        refused.is_err(),
        "owned_command.actor_kind 不得接受认领方：{refused:?}"
    );
    pool.close().await;
}

/// §7.2 与 spec 的「升级中途失败整体回滚」：v1 → v2 的 DDL、12-step 重建与 Import 拆分都在**同一
/// 事务**内，因此第二段脚本失败时第一段的建表与重建也必须回滚——库要么是完整的 v1，要么是完整的
/// v2，不存在「管理表已建、审计 CHECK 未换」的半升级状态；去掉故障后重新打开必须能升级成功。
#[tokio::test]
async fn a_failed_upgrade_rolls_back_to_v1() {
    let dir = temp_dir("migrate-failed-upgrade");
    let path = copy_fixture("from-v1.sqlite3", &dir);

    // 注入：占住**第二段**升级脚本要建的表名（`imported_import_v2`），使失败发生在第一段之后。
    let pool = raw_write_pool(&path).await;
    sqlx::query("CREATE TABLE imported_import_v2 (placeholder TEXT)")
        .execute(&pool)
        .await
        .expect("inject conflicting table");
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .expect("checkpoint");
    pool.close().await;

    let failed = SqliteStore::open(still_writable(&dir), &at()).await;
    assert!(failed.is_err(), "注入的 DDL 冲突必须让升级失败");
    drop(failed);

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        1,
        "失败后不得写版本"
    );
    assert_eq!(
        meta_rows(&pool)
            .await
            .iter()
            .find(|(key, _)| key == "owned_schema_version")
            .map(|(_, value)| value.as_str()),
        Some("1"),
        "失败后两族版本保持 v1"
    );
    let tables = texts(
        &pool,
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
    .await;
    assert!(
        !tables.iter().any(|name| name == "owned_device"),
        "DDL 常量建的 v2 管理表必须随事务回滚：{tables:?}"
    );
    assert!(!tables.iter().any(|name| name == "imported_import_export"));
    let audit_ddl_after_failure = texts(
        &pool,
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'owned_audit'",
    )
    .await;
    assert!(
        audit_ddl_after_failure
            .first()
            .is_some_and(|sql| !sql.contains("provider.configured")),
        "审计表的 12-step 重建必须随事务回滚"
    );
    let import_ddl = texts(
        &pool,
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'imported_import'",
    )
    .await;
    assert!(
        import_ddl
            .first()
            .is_some_and(|sql| sql.contains("export_id")),
        "Import 管理行必须仍是 v1 形状"
    );
    pool.close().await;

    // 去掉注入后重新打开必须升级成功（半升级状态不存在）。
    let pool = raw_write_pool(&path).await;
    sqlx::query("DROP TABLE imported_import_v2")
        .execute(&pool)
        .await
        .expect("drop injection");
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .expect("checkpoint");
    pool.close().await;

    let store = SqliteStore::open(still_writable(&dir), &at())
        .await
        .expect("retry must upgrade");
    assert_eq!(store.metadata().owned_schema_version, 3);
    assert_eq!(store.metadata().imported_schema_version, 3);
    store.close().await;
}

/// §9.1/§9.28：升级后的库第二次打开必须**跳过**升级步骤，因此一切逐字节不变。
#[tokio::test]
async fn second_open_of_an_upgraded_database_rewrites_nothing() {
    let dir = temp_dir("migrate-upgraded-idempotent");
    let path = copy_fixture("from-v1.sqlite3", &dir);

    let store = SqliteStore::open(still_writable(&dir), &at())
        .await
        .expect("first open upgrades");
    store.close().await;

    let pool = raw_pool(&path).await;
    let schema = schema_sql(&pool).await;
    let meta = meta_rows(&pool).await;
    let version = scalar_i64(&pool, "PRAGMA user_version").await;
    let mut rows = Vec::new();
    for table in TABLES {
        rows.push((*table, table_snapshot(&pool, table).await));
    }
    pool.close().await;

    let store = SqliteStore::open(still_writable(&dir), &at())
        .await
        .expect("second open must be a no-op");
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(schema_sql(&pool).await, schema, "second open rewrote DDL");
    assert_eq!(meta_rows(&pool).await, meta, "second open rewrote meta");
    assert_eq!(
        scalar_i64(&pool, "PRAGMA user_version").await,
        version,
        "second open changed user_version"
    );
    for (table, before) in rows {
        assert_eq!(
            table_snapshot(&pool, table).await,
            before,
            "second open changed rows in {table}"
        );
    }
    pool.close().await;
}

/// §8：损坏的数据库文件不得让 `open` panic。两种可接受结局：返回具名错误，或进入只读失败关闭
/// （`health().read_only == true`，写路径返回 `PortError::Corrupt`）；库若仍可读，`read_view()` 可用。
#[tokio::test]
async fn corrupt_database_fails_closed() {
    use acp_core::ports::{OwnedCommit, SessionStore};
    use storage_sqlite::session_store::SqliteStore;
    use support::*;

    let dir = temp_dir("migrate-corrupt");
    let path = copy_fixture("empty.sqlite3", &dir);
    // 把文件头（前 100 字节）写成与之无关的字节，破坏 SQLite 头（magic/page size）。
    let mut bytes = std::fs::read(&path).expect("read fixture");
    for (index, byte) in bytes.iter_mut().take(100).enumerate() {
        *byte = u8::try_from(index % 251).expect("byte");
    }
    std::fs::write(&path, &bytes).expect("write corrupt fixture");

    let opened = SqliteStore::open(StorageConfig::new(&dir), &at()).await;
    match opened {
        Err(error) => {
            // 具名错误（不是 panic）：文件格式/损坏/底层 sqlite 错误三者之一。
            assert!(
                matches!(
                    error,
                    StorageError::FileFormatTooNew { .. }
                        | StorageError::Corrupt(_)
                        | StorageError::Sql(_)
                ),
                "a corrupt file must fail with a named error, got {error}"
            );
        }
        Ok(store) => {
            let health = store.health().await.expect("health");
            assert!(
                health.read_only && !health.integrity_ok,
                "a partially readable corrupt file must enter read-only fail-closed mode"
            );
            let error = store
                .commit(OwnedCommit {
                    session: None,
                    at: at(),
                    expected_version: None,
                    state: None,
                    turns: Vec::new(),
                    events: Vec::new(),
                    interactions: Vec::new(),
                    compacted: Vec::new(),
                    idempotency: None,
                    command_terminal: None,
                    origin_epoch: None,
                })
                .await
                .expect_err("write paths must be refused in fail-closed mode");
            assert!(
                matches!(error, acp_core::model::PortError::Corrupt(_)),
                "expected PortError::Corrupt, got {error}"
            );
            // 只读查询继续（§8）。
            store.read_view().await.expect("read view must still work");
            store.close().await;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 夹具生成器
// ---------------------------------------------------------------------------------------------

/// `from-v1.sqlite3` 里的固定标识（升级断言按这些值逐行比对）。
const FIXTURE_NODE: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const FIXTURE_EXPORT: &str = "export-fixture";
const FIXTURE_SESSION: &str = "11111111-1111-4111-8111-111111111111";
const FIXTURE_TURN: &str = "22222222-2222-4222-8222-222222222222";
const FIXTURE_EPOCH: &str = "33333333-3333-4333-8333-333333333333";
const FIXTURE_EVENT_ONE: &str = "44444444-4444-4444-8444-444444444444";
const FIXTURE_EVENT_TWO: &str = "55555555-5555-4555-8555-555555555555";
const FIXTURE_EVENT_THREE: &str = "66666666-6666-4666-8666-666666666666";
const FIXTURE_REQUEST: &str = "77777777-7777-4777-8777-777777777777";
const FIXTURE_IMPORT: &str = "import-fixture";
const FIXTURE_IMPORT_REQUEST: &str = "88888888-8888-4888-8888-888888888888";
const FIXTURE_LAST_ORIGIN_SEQUENCE: i64 = 7;
const FIXTURE_ACKED_ORIGIN_SEQUENCE: i64 = 5;
const FIXTURE_LOCAL_SEQUENCE: i64 = 1;
/// 配对 id（`pairing_claimant` 审计行的 `actor_id`；column 上无 CHECK，但保持与产品同形状）。
const FIXTURE_PAIRING: &str = "99999999-9999-4999-8999-999999999999";

/// 重建 `fixtures/storage/v2/from-v1.sqlite3`（**v1 形状**的升级输入）。默认忽略；重建方式：
///
/// ```text
/// cargo test -p storage-sqlite --test migration -- --ignored regenerate_v1_fixture
/// ```
///
/// - `from-v1.sqlite3`：**v1 历史夹具** `fixtures/storage/v1/empty.sqlite3` 的副本 + 一组带
///   会话/事件/cursor/幂等/审计数据的 v1 行，保持 `user_version = 1`；`owned_audit` 故意留下
///   `audit_id = 1,2,5` 的空洞（写入 1..=7 后删掉 3/4/6/7，因此 `sqlite_sequence.seq = 7` 真正领先于
///   `max(audit_id) = 5`；`imported_audit` 同款：1/2 与 `seq = 3`），升级用例据此断言序列不回退。
///
/// 同目录的另两件是**冻结的历史资产**，当前二进制不再能生成它们：
///
/// - `empty.sqlite3` 是 v2 形状的空库（v2 → v3 升级用例的输入）——v3 之后 `open` 只会建出 v3 形状，
///   因此它只是「升级输入」，不再是「当前版本」的代表；
/// - `too-new.sqlite3`（`user_version = 3`）在 v3 之后不再「过新」，过新用例改为把 `empty.sqlite3`
///   的临时副本顶到 `FILE_FORMAT_VERSION + 1`。
#[tokio::test]
#[ignore = "夹具生成器：只在需要重建 fixtures/storage/v2/from-v1.sqlite3 时手动运行"]
async fn regenerate_v1_fixture() {
    let fixtures = repo_path("fixtures/storage/v2");
    std::fs::create_dir_all(&fixtures).expect("create fixtures/storage/v2");

    // v1 库：以历史夹具为基底插入 v1 形状的行。
    let dir = temp_dir("fixture-v2-from-v1");
    let path = copy_fixture_from("v1", "empty.sqlite3", &dir);
    let pool = raw_write_pool(&path).await;

    sqlx::query(
        "INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, origin_epoch, \
         version, created_at, updated_at) \
         VALUES (?1, 'fixture session', 'fixture-agent', 'Fixture Agent', 'idle', ?2, 1, ?3, ?3)",
    )
    .bind(FIXTURE_SESSION)
    .bind(FIXTURE_EPOCH)
    .bind(at().as_str())
    .execute(&pool)
    .await
    .expect("owned_session");

    sqlx::query(
        "INSERT INTO owned_turn (turn_id, session_id, state, queue_index, started_at, ended_at) \
         VALUES (?1, ?2, 'completed', 0, ?3, ?3)",
    )
    .bind(FIXTURE_TURN)
    .bind(FIXTURE_SESSION)
    .bind(at().as_str())
    .execute(&pool)
    .await
    .expect("owned_turn");

    // 三条事件：delta/delta/final_message，序号与会话内序号都是 1/2/3（升级不得重编号）。
    let views = [
        r#"{"text":"fixture one","type":"text"}"#,
        r#"{"text":"fixture two","type":"text"}"#,
        r#"{"text":"fixture final","type":"text"}"#,
    ];
    let (kinds, policies) = (
        ["delta", "delta", "final_message"],
        ["short_term", "short_term", "durable"],
    );
    for (index, ((view, kind), policy)) in views.iter().zip(kinds).zip(policies).enumerate() {
        let sequence = i64::try_from(index).expect("index") + 1;
        let event_id = [FIXTURE_EVENT_ONE, FIXTURE_EVENT_TWO, FIXTURE_EVENT_THREE][index];
        let expires = if policy == "durable" {
            None
        } else {
            Some("2026-09-19T00:00:00.000Z")
        };
        sqlx::query(
            "INSERT INTO owned_event (global_sequence, session_id, session_sequence, origin_epoch, \
             origin_sequence, event_id, turn_id, event_type, kind, policy, origin_kind, payload_json, \
             payload_digest, created_at, expires_at) \
             VALUES (?1, ?2, ?1, ?3, ?1, ?4, ?5, 'agent.message', ?6, ?7, 'agent', ?8, ?9, ?10, ?11)",
        )
        .bind(sequence)
        .bind(FIXTURE_SESSION)
        .bind(FIXTURE_EPOCH)
        .bind(event_id)
        .bind(FIXTURE_TURN)
        .bind(kind)
        .bind(policy)
        .bind(*view)
        .bind(cj1_digest(view))
        .bind(at().as_str())
        .bind(expires)
        .execute(&pool)
        .await
        .expect("owned_event");
    }

    sqlx::query(
        "INSERT INTO owned_command (actor_kind, actor_id, request_id, session_id, command, kind, \
         request_fingerprint, accepted_at, status, terminal_at, terminal_event_id) \
         VALUES ('device', ?1, ?2, ?3, 'session.prompt', 'mutation', ?4, ?5, 'completed', ?5, ?6)",
    )
    .bind(FIXTURE_NODE)
    .bind(FIXTURE_REQUEST)
    .bind(FIXTURE_SESSION)
    .bind(digest_text("fixture prompt"))
    .bind(at().as_str())
    .bind(FIXTURE_EVENT_ONE)
    .execute(&pool)
    .await
    .expect("owned_command");

    // 审计：先写 7 行再删掉 3/4/6/7，留下 1/2/5 —— 序列（7）因此**领先于** max(audit_id) = 5。
    // 这是判据 28「`AUTOINCREMENT` 序列不回退」唯一的判别力来源：12-step 重建若只按现存行取
    // `max(audit_id)` 就会把序列压回 5，只有序列仍领先才证明 `restore_audit_sequences` 真的生效。
    for (audit_id, action) in [
        (1_i64, "device.authenticated"),
        (2, "node.paired"),
        (3, "authorization.denied"),
        (4, "rate_limit.triggered"),
        (5, "device.scopes_changed"),
        (6, "device.revoked"),
        (7, "device.auth_failed"),
    ] {
        sqlx::query(
            "INSERT INTO owned_audit (audit_id, at, action, actor_kind, actor_id, target_kind, \
             target_id, outcome) VALUES (?1, ?2, ?3, 'device', ?4, 'device', ?4, 'success')",
        )
        .bind(audit_id)
        .bind(at().as_str())
        .bind(action)
        .bind(FIXTURE_NODE)
        .execute(&pool)
        .await
        .expect("owned_audit");
    }
    sqlx::query("DELETE FROM owned_audit WHERE audit_id IN (3, 4, 6, 7)")
        .execute(&pool)
        .await
        .expect("prune audit rows");

    // Import 家族：v1 形状的 imported_import（带 export_id）+ cursor/交付索引/命令引用/审计。
    sqlx::query(
        "INSERT INTO imported_import (import_id, owner_node_id, export_id, display_name, endpoint_ref, \
         cache_policy, owner_server_epoch, created_at) \
         VALUES (?1, ?2, ?3, 'Fixture Export', 'wss://owner.invalid', 'no-content-cache', ?4, ?5)",
    )
    .bind(FIXTURE_IMPORT)
    .bind(FIXTURE_NODE)
    .bind(FIXTURE_EXPORT)
    .bind(FIXTURE_EPOCH)
    .bind(at().as_str())
    .execute(&pool)
    .await
    .expect("imported_import");

    sqlx::query(
        "INSERT INTO imported_session (owner_node_id, export_id, session_id, title, state, version, \
         last_origin_epoch, last_origin_sequence, acked_origin_epoch, acked_origin_sequence, \
         next_local_sequence, updated_at) \
         VALUES (?1, ?2, ?3, 'Remote session', 'idle', 1, ?4, ?5, ?4, ?6, ?7, ?8)",
    )
    .bind(FIXTURE_NODE)
    .bind(FIXTURE_EXPORT)
    .bind(FIXTURE_SESSION)
    .bind(FIXTURE_EPOCH)
    .bind(FIXTURE_LAST_ORIGIN_SEQUENCE)
    .bind(FIXTURE_ACKED_ORIGIN_SEQUENCE)
    .bind(FIXTURE_LOCAL_SEQUENCE + 1)
    .bind(at().as_str())
    .execute(&pool)
    .await
    .expect("imported_session");

    sqlx::query(
        "INSERT INTO imported_delivery_index (owner_node_id, export_id, session_id, origin_event_id, \
         origin_epoch, origin_sequence, local_sequence, event_type, payload_digest, received_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, 'resource.event', ?7, ?8)",
    )
    .bind(FIXTURE_NODE)
    .bind(FIXTURE_EXPORT)
    .bind(FIXTURE_SESSION)
    .bind(FIXTURE_EVENT_ONE)
    .bind(FIXTURE_EPOCH)
    .bind(FIXTURE_LOCAL_SEQUENCE)
    .bind(digest_text("fixture remote payload"))
    .bind(at().as_str())
    .execute(&pool)
    .await
    .expect("imported_delivery_index");

    sqlx::query(
        "INSERT INTO imported_command_ref (owner_node_id, export_id, session_id, request_id, command, \
         status, accepted_at) VALUES (?1, ?2, ?3, ?4, 'session.prompt', 'accepted', ?5)",
    )
    .bind(FIXTURE_NODE)
    .bind(FIXTURE_EXPORT)
    .bind(FIXTURE_SESSION)
    .bind(FIXTURE_IMPORT_REQUEST)
    .bind(at().as_str())
    .execute(&pool)
    .await
    .expect("imported_command_ref");

    // imported 侧同理：写 1..=3 再删掉 3，留下 1/2 而序列为 3（同样领先于 max(audit_id)）。
    for (audit_id, action) in [
        (1_i64, "node.trust_revoked"),
        (2, "authorization.denied"),
        (3, "rate_limit.triggered"),
    ] {
        sqlx::query(
            "INSERT INTO imported_audit (audit_id, at, action, actor_kind, actor_id, owner_node_id, \
             export_id, session_id, target_kind, target_id, outcome) \
             VALUES (?1, ?2, ?3, 'node', ?4, ?4, ?5, ?6, 'node', ?4, 'success')",
        )
        .bind(audit_id)
        .bind(at().as_str())
        .bind(action)
        .bind(FIXTURE_NODE)
        .bind(FIXTURE_EXPORT)
        .bind(FIXTURE_SESSION)
        .execute(&pool)
        .await
        .expect("imported_audit");
    }
    sqlx::query("DELETE FROM imported_audit WHERE audit_id = 3")
        .execute(&pool)
        .await
        .expect("prune imported audit rows");

    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await
        .expect("checkpoint");
    pool.close().await;
    std::fs::copy(&path, fixtures.join("from-v1.sqlite3")).expect("write from-v1.sqlite3");
}

/// `payload_digest` 的口径与存储层一致：`ACPR-CJ1` 规范化后再取 SHA-256 的规范 base64url（§9.9）。
fn cj1_digest(view: &str) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    let canonical = acpr_wire::cj1::canonicalize(view).expect("canonical view");
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(canonical.as_bytes()))
}
