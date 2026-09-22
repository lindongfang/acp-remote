//! `storage-sqlite` 集成测试的公共辅助：定位仓库根、临时目录、时间戳与原始 SQL 连接。
//!
//! 路径一律通过 `CARGO_MANIFEST_DIR` 向上寻找 `fixtures/storage/v1`，**不依赖进程 cwd**。

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use storage_sqlite::migrate::StorageConfig;

/// 测试用的固定时间戳（本 crate 不读系统时间，测试也不依赖真实时钟）。
pub const AT: &str = "2026-09-18T00:00:00.000Z";

/// 规范摘要文本：`base64url(SHA-256(seed))`（43 字符、无填充、末字符的低 2 位为 0）。
///
/// 手写常量极易写出**非规范**的尾字符（例如以 `B`/`F` 结尾），因此测试一律用本函数生成摘要。
pub fn digest_text(seed: &str) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(seed.as_bytes()))
}

/// 仓库根：含 `fixtures/storage/v1` 的那一级目录。
pub fn repo_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("fixtures/storage/v1").is_dir() {
            return dir;
        }
        if !dir.pop() {
            panic!("repository root not found from CARGO_MANIFEST_DIR");
        }
    }
}

pub fn repo_path(relative: &str) -> PathBuf {
    repo_root().join(relative)
}

/// 每个测试独立的临时数据目录。
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("acpr-storage-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // §7.1：正式模式对已存在的宽松数据目录失败关闭（见 `migrate`）。测试预创建的目录必须与产品
    // 创建时一致（`0700`），否则 Linux runner 在 umask 022 下会先造出一个 `0755` 目录而被拒。
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .expect("create temp dir");
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// 把夹具库复制到临时数据目录，并**落在 `SqliteStore` 实际打开的路径上**
/// （`<data_dir>/acp-remote.sqlite3`）：否则 store 会新建一个空库，夹具根本没被打开。
pub fn copy_fixture(name: &str, into: &Path) -> PathBuf {
    let source = repo_path(&format!("fixtures/storage/v1/{name}"));
    let target = into.join(storage_sqlite::migrate::DATABASE_FILE);
    std::fs::copy(&source, &target).unwrap_or_else(|error| panic!("copy {source:?}: {error}"));
    target
}

/// 测试用的存储配置：默认值 + 覆盖点。
pub fn config(dir: &Path) -> StorageConfig {
    StorageConfig::new(dir)
}

/// 直接连库的只读池（测试用来做「逐行快照」与黄金列清单检查）。
pub async fn raw_pool(path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open raw pool")
}

pub async fn raw_write_pool(path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open raw write pool")
}

/// 某张表的全部行，按主键排序后渲染成文本（逐行比较用的快照）。
pub async fn table_snapshot(pool: &SqlitePool, table: &str) -> Vec<String> {
    let rows = sqlx::query(&format!("SELECT * FROM {table} ORDER BY rowid"))
        .fetch_all(pool)
        .await
        .unwrap_or_else(|error| panic!("snapshot {table}: {error}"));
    rows.iter()
        .map(|row| {
            let mut cells = Vec::new();
            for index in 0..row.len() {
                let value = row
                    .try_get::<Option<String>, _>(index)
                    .map(|text| text.unwrap_or_else(|| "∅".to_owned()))
                    .unwrap_or_else(|_| {
                        row.try_get::<Option<i64>, _>(index)
                            .map(|number| number.map_or_else(|| "∅".to_owned(), |n| n.to_string()))
                            .unwrap_or_else(|_| "?".to_owned())
                    });
                cells.push(value);
            }
            cells.join("|")
        })
        .collect()
}

/// `sqlite_master` 里每条对象的 SQL 文本（§9.1 的逐字节幂等判据）。
pub async fn schema_sql(pool: &SqlitePool) -> Vec<(String, String, String)> {
    let rows = sqlx::query(
        "SELECT type, name, COALESCE(sql, '') AS sql FROM sqlite_master ORDER BY type, name",
    )
    .fetch_all(pool)
    .await
    .expect("sqlite_master");
    rows.iter()
        .map(|row| {
            (
                row.try_get::<String, _>("type").expect("type"),
                row.try_get::<String, _>("name").expect("name"),
                row.try_get::<String, _>("sql").expect("sql"),
            )
        })
        .collect()
}

/// `meta` 的全部键值（排序）。
pub async fn meta_rows(pool: &SqlitePool) -> Vec<(String, String)> {
    let rows = sqlx::query("SELECT key, value FROM meta ORDER BY key")
        .fetch_all(pool)
        .await
        .expect("meta");
    rows.iter()
        .map(|row| {
            (
                row.try_get::<String, _>("key").expect("key"),
                row.try_get::<String, _>("value").expect("value"),
            )
        })
        .collect()
}

pub async fn scalar_i64(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|error| panic!("{sql}: {error}"))
}

/// §9.6 的黄金列清单：`imported_*` 的列名集合必须与合同逐项相等（顺序敏感，便于发现漂移）。
pub async fn column_names(pool: &SqlitePool, table: &str) -> Vec<String> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .unwrap_or_else(|error| panic!("table_info {table}: {error}"));
    rows.iter()
        .map(|row| row.try_get::<String, _>("name").expect("name"))
        .collect()
}

/// 与 `session_store::measure_total` **同口径**的度量：12 张表的所有 TEXT 列 `length()` 之和
/// + `owned_attachment.byte_length` 之和（列清单从 `pragma_table_info` 取，不手抄）。
///
/// 容量测试的预算一律由它派生（`measured + 半条事件`），不拍数字——度量口径一变，测试自动跟上。
pub async fn measured_storage_bytes(pool: &SqlitePool) -> u64 {
    // 与存储层的 `build_measure_sql` 同一算法：从 schema 现读两张家族的全部 TEXT 列 + 附件字节。
    // 两端一致由 `capacity_measure_matches_the_store` 断言（单靠"看起来一样"曾经漂移过）。
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("tables");
    let mut terms = Vec::new();
    for table in tables {
        let columns: Vec<(String, String)> =
            sqlx::query_as("SELECT name, type FROM pragma_table_info(?1)")
                .bind(&table)
                .fetch_all(pool)
                .await
                .expect("columns");
        let text_columns: Vec<String> = columns
            .into_iter()
            .filter(|(_, kind)| kind.eq_ignore_ascii_case("TEXT"))
            .map(|(name, _)| format!("COALESCE(length(\"{name}\"), 0)"))
            .collect();
        if text_columns.is_empty() {
            continue;
        }
        terms.push(format!(
            "COALESCE((SELECT SUM({}) FROM \"{table}\"), 0)",
            text_columns.join(" + ")
        ));
    }
    terms.push("COALESCE((SELECT SUM(byte_length) FROM owned_attachment), 0)".to_owned());
    let total: i64 = sqlx::query_scalar(&format!("SELECT {}", terms.join(" + ")))
        .fetch_one(pool)
        .await
        .expect("measure storage");
    u64::try_from(total).unwrap_or(0)
}
