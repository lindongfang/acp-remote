//! `docs/CORE_PORTS_AND_STORAGE.md` §7.1/§7.2/§7.3/§7.4：文件布局、PRAGMA、连接模型、`owned_*`/
//! `imported_*` 表结构与 migration。
//!
//! 本模块只依赖 `sqlx` 与 `std`——不引用 `core` 的任何类型。表结构常量是合同 §7.3/§7.4 的逐字转录，
//! 改动必须同时改合同。migration 的键与值都以 `TEXT` 形式存在于 `meta`，时间值由调用方以
//! `&str` 传入（§2：`storage-sqlite` 内部不得读系统时间）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Executor, Sqlite};

use crate::error::StorageError;

/// §7.2：`PRAGMA user_version` = 文件格式版本。
pub const FILE_FORMAT_VERSION: i64 = 1;
/// §7.2：`meta.owned_schema_version` 的已知版本。
pub const OWNED_SCHEMA_VERSION: i64 = 1;
/// §7.2：`meta.imported_schema_version` 的已知版本。
pub const IMPORTED_SCHEMA_VERSION: i64 = 1;

/// §7.1：单文件 `<data_dir>/acp-remote.sqlite3`。
pub const DATABASE_FILE: &str = "acp-remote.sqlite3";
/// §7.1：附件目录默认 `<data_dir>/attachments`。
pub const ATTACHMENTS_DIR: &str = "attachments";

/// §7.2 的 `meta` 必需键（§7.3）。
pub const META_SERVER_EPOCH: &str = "server_epoch";
pub const META_OWNED_SCHEMA_VERSION: &str = "owned_schema_version";
pub const META_IMPORTED_SCHEMA_VERSION: &str = "imported_schema_version";
pub const META_CREATED_AT: &str = "created_at";
pub const META_LAST_PRUNE_AT: &str = "last_prune_at";

/// §7.1：`busy_timeout = 5000`（毫秒）。
pub const BUSY_TIMEOUT_MS: u64 = 5_000;
/// §7.1：`wal_autocheckpoint = 1000`（页）。
pub const WAL_AUTOCHECKPOINT_PAGES: u32 = 1_000;
/// 默认只读连接数。§7.1 只要求「1 写 + N 读」，N 的取值属实现细节。
pub const DEFAULT_READ_CONNECTIONS: u32 = 4;

/// §7.3 的 `owned_*` 表与索引（含 §7.3 的 `meta`）。
///
/// `IF NOT EXISTS` 是 migration 幂等的实现方式（§7.2：「可重复执行」）：已存在的表不会被重写，
/// 因此 `sqlite_master.sql` 的文本在第二次启动后逐字节不变（§9.1）。
pub const OWNED_SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS owned_session (
  session_id        TEXT PRIMARY KEY,
  title             TEXT,
  agent_id          TEXT NOT NULL,
  agent_name        TEXT NOT NULL,
  state             TEXT NOT NULL CHECK (state IN ('idle','queued','running','waiting_input','waiting_permission','failed','closed')),
  origin_epoch      TEXT NOT NULL,
  current_mode_id   TEXT,
  current_mode_name TEXT,
  version           INTEGER NOT NULL,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  closed_at         TEXT
) STRICT;

CREATE TABLE IF NOT EXISTS owned_turn (
  turn_id     TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  state       TEXT NOT NULL CHECK (state IN ('queued','running','waiting_input','waiting_permission','completed','failed','cancelled')),
  queue_index INTEGER NOT NULL,
  causation   TEXT,
  started_at  TEXT,
  ended_at    TEXT,
  UNIQUE (session_id, queue_index)
) STRICT;

CREATE TABLE IF NOT EXISTS owned_event (
  global_sequence  INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id       TEXT REFERENCES owned_session(session_id) ON DELETE CASCADE,
  session_sequence INTEGER,
  origin_epoch     TEXT,
  origin_sequence  INTEGER,
  event_id         TEXT NOT NULL UNIQUE,
  turn_id          TEXT REFERENCES owned_turn(turn_id),
  event_type       TEXT NOT NULL,
  kind             TEXT NOT NULL CHECK (kind IN ('state','delta','final_message','structured','interaction','summary')),
  policy           TEXT NOT NULL CHECK (policy IN ('durable','short_term')),
  origin_kind      TEXT NOT NULL CHECK (origin_kind IN ('agent','device','daemon','local_cli')),
  causation        TEXT,
  payload_json     TEXT NOT NULL,
  payload_digest   TEXT NOT NULL,
  acp_media_type   TEXT,
  acp_raw_json     TEXT,
  acp_byte_length  INTEGER,
  acp_sha256       TEXT,
  acp_raw_unavailable_reason TEXT CHECK (acp_raw_unavailable_reason IN ('size_limit','retention_expired','storage_failure')),
  created_at       TEXT NOT NULL,
  expires_at       TEXT,
  compacted_into   INTEGER,
  CHECK (session_id IS NOT NULL OR session_sequence IS NULL),
  CHECK ((session_id IS NULL) = (origin_epoch IS NULL)),
  CHECK ((session_id IS NULL) = (origin_sequence IS NULL)),
  CHECK (acp_raw_unavailable_reason IS NOT NULL
         OR (acp_raw_json IS NULL) = (acp_sha256 IS NULL)),
  CHECK (acp_raw_unavailable_reason IS NULL OR acp_raw_json IS NULL),
  CHECK (acp_raw_json IS NOT NULL OR acp_raw_unavailable_reason IS NOT NULL OR acp_sha256 IS NULL)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS owned_event_session ON owned_event(session_id, session_sequence);
CREATE UNIQUE INDEX IF NOT EXISTS owned_event_origin ON owned_event(session_id, origin_epoch, origin_sequence) WHERE session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS owned_event_expiry ON owned_event(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS owned_event_turn ON owned_event(session_id, turn_id, kind);

CREATE TABLE IF NOT EXISTS owned_command (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  actor_kind    TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id      TEXT NOT NULL,
  request_id    TEXT NOT NULL,
  session_id    TEXT REFERENCES owned_session(session_id) ON DELETE CASCADE,
  command       TEXT NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('query','mutation')),
  expected_version TEXT,
  request_fingerprint TEXT NOT NULL,
  accepted_at   TEXT,
  status        TEXT NOT NULL CHECK (status IN ('accepted','completed','failed','rejected','uncertain')),
  terminal_at   TEXT,
  terminal_event_id TEXT,
  result_json   TEXT,
  error_code    TEXT, error_message TEXT, error_details_json TEXT, retryable INTEGER,
  UNIQUE (actor_kind, actor_id, request_id),
  CHECK (status = 'rejected' OR accepted_at IS NOT NULL),
  CHECK (status <> 'rejected' OR result_json IS NULL),
  CHECK (status <> 'completed' OR error_code IS NULL),
  CHECK (status IN ('accepted','completed') OR error_code IS NOT NULL),
  CHECK (kind <> 'query' OR terminal_event_id IS NULL),
  CHECK (status <> 'completed' OR kind <> 'mutation' OR terminal_event_id IS NOT NULL),
  CHECK (status NOT IN ('failed','uncertain') OR terminal_event_id IS NOT NULL)
) STRICT;
CREATE INDEX IF NOT EXISTS owned_command_status ON owned_command(status) WHERE status IN ('uncertain','accepted');
CREATE INDEX IF NOT EXISTS owned_command_session ON owned_command(session_id, accepted_at);

CREATE TABLE IF NOT EXISTS owned_interaction (
  interaction_id  TEXT PRIMARY KEY,
  session_id      TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  kind            TEXT NOT NULL CHECK (kind IN ('permission','elicitation')),
  request_event   INTEGER REFERENCES owned_event(global_sequence),
  state           TEXT NOT NULL CHECK (state IN ('pending','resolved','expired')),
  created_at      TEXT NOT NULL,
  resolved_at     TEXT,
  decision_option_id TEXT,
  decision_kind   TEXT CHECK (decision_kind IN ('allow_once','allow_always','reject_once','reject_always')),
  elicitation_action TEXT CHECK (elicitation_action IN ('submit','decline','cancel')),
  elicitation_values_json TEXT,
  resolved_by_kind TEXT, resolved_by_id TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS owned_interaction_pending ON owned_interaction(session_id, state) WHERE state = 'pending';

CREATE TABLE IF NOT EXISTS owned_audit (
  audit_id     INTEGER PRIMARY KEY AUTOINCREMENT,
  at           TEXT NOT NULL,
  action       TEXT NOT NULL CHECK (action IN (
                 'pairing.created','pairing.claimed','pairing.approved','pairing.rejected','pairing.expired',
                 'device.authenticated','device.auth_failed','device.revoked','device.scopes_changed',
                 'node.paired','node.trust_revoked','node.identity_changed',
                 'authorization.denied','rate_limit.triggered','storage.integrity_failed')),
  actor_kind   TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id     TEXT NOT NULL,
  via_node_id  TEXT,
  local_principal_ref TEXT,
  target_kind  TEXT NOT NULL, target_id TEXT NOT NULL,
  outcome      TEXT NOT NULL CHECK (outcome IN ('success','denied','failed')),
  detail_digest TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS owned_audit_at ON owned_audit(at);
CREATE INDEX IF NOT EXISTS owned_audit_action ON owned_audit(action, at);

CREATE TABLE IF NOT EXISTS owned_attachment (
  attachment_id TEXT NOT NULL UNIQUE,
  sha256      TEXT PRIMARY KEY,
  byte_length INTEGER NOT NULL,
  media_type  TEXT NOT NULL,
  relative_path TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  last_used_at TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS owned_attachment_link (
  session_id   TEXT NOT NULL REFERENCES owned_session(session_id) ON DELETE CASCADE,
  attachment_id TEXT NOT NULL REFERENCES owned_attachment(attachment_id),
  generation   INTEGER NOT NULL,
  sha256       TEXT NOT NULL REFERENCES owned_attachment(sha256),
  PRIMARY KEY (session_id, attachment_id)
) STRICT;
"#;

/// §7.4 的 `imported_*` 无正文表（`no-content-cache`）。
///
/// 列集合是合同冻结的**黄金列清单**：`tests/no_content_columns.rs` 用 `PRAGMA table_info` 与之逐项
/// 比对，新增列即失败（§9.6）。
pub const IMPORTED_SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS imported_import (
  import_id     TEXT PRIMARY KEY,
  owner_node_id TEXT NOT NULL,
  export_id     TEXT NOT NULL,
  display_name  TEXT,
  endpoint_ref  TEXT,
  cache_policy  TEXT NOT NULL CHECK (cache_policy = 'no-content-cache'),
  owner_server_epoch TEXT,
  created_at    TEXT NOT NULL,
  removed_at    TEXT,
  UNIQUE (owner_node_id, export_id)
) STRICT;

CREATE TABLE IF NOT EXISTS imported_session (
  owner_node_id  TEXT NOT NULL,
  export_id      TEXT NOT NULL,
  session_id     TEXT NOT NULL,
  title          TEXT,
  agent_id       TEXT, agent_name TEXT,
  state          TEXT,
  version        INTEGER,
  created_at     TEXT,
  last_origin_epoch   TEXT,
  last_origin_sequence INTEGER,
  acked_origin_epoch  TEXT,
  acked_origin_sequence INTEGER,
  next_local_sequence INTEGER NOT NULL DEFAULT 1,
  attachment_id TEXT,
  attachment_generation INTEGER,
  updated_at     TEXT NOT NULL,
  PRIMARY KEY (owner_node_id, export_id, session_id)
) STRICT;

CREATE TABLE IF NOT EXISTS imported_delivery_index (
  owner_node_id   TEXT NOT NULL,
  export_id       TEXT NOT NULL,
  session_id      TEXT NOT NULL,
  origin_event_id TEXT NOT NULL,
  origin_epoch    TEXT NOT NULL,
  origin_sequence INTEGER NOT NULL,
  local_sequence  INTEGER NOT NULL,
  event_type      TEXT NOT NULL,
  payload_digest  TEXT NOT NULL,
  received_at     TEXT NOT NULL,
  PRIMARY KEY (owner_node_id, export_id, session_id, origin_event_id),
  UNIQUE (owner_node_id, export_id, session_id, local_sequence),
  UNIQUE (owner_node_id, export_id, session_id, origin_epoch, origin_sequence),
  FOREIGN KEY (owner_node_id, export_id, session_id)
    REFERENCES imported_session(owner_node_id, export_id, session_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS imported_command_ref (
  owner_node_id  TEXT NOT NULL, export_id TEXT NOT NULL, session_id TEXT NOT NULL,
  request_id     TEXT NOT NULL,
  command        TEXT NOT NULL,
  status         TEXT NOT NULL CHECK (status IN ('accepted','completed','failed','rejected','uncertain')),
  accepted_at    TEXT,
  terminal_at    TEXT, terminal_event_id TEXT,
  error_code     TEXT, retryable INTEGER,
  PRIMARY KEY (owner_node_id, export_id, session_id, request_id),
  CHECK (status = 'rejected' OR accepted_at IS NOT NULL),
  FOREIGN KEY (owner_node_id, export_id, session_id)
    REFERENCES imported_session(owner_node_id, export_id, session_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS imported_audit (
  audit_id     INTEGER PRIMARY KEY AUTOINCREMENT,
  at           TEXT NOT NULL,
  action       TEXT NOT NULL CHECK (action IN (
                 'pairing.created','pairing.claimed','pairing.approved','pairing.rejected','pairing.expired',
                 'device.authenticated','device.auth_failed','device.revoked','device.scopes_changed',
                 'node.paired','node.trust_revoked','node.identity_changed',
                 'authorization.denied','rate_limit.triggered','storage.integrity_failed')),
  actor_kind   TEXT NOT NULL CHECK (actor_kind IN ('device','node','cli')),
  actor_id     TEXT NOT NULL,
  owner_node_id TEXT, export_id TEXT, session_id TEXT,
  request_id   TEXT,
  local_principal_ref TEXT,
  target_kind  TEXT NOT NULL, target_id TEXT NOT NULL,
  outcome      TEXT NOT NULL CHECK (outcome IN ('success','denied','failed')),
  detail_digest TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS imported_audit_at ON imported_audit(at);
"#;

/// §7.5 的存储配置键。默认值逐项对应合同表格。
///
/// 配置加载属 `app`；本结构只承载 `storage-sqlite` 需要知道的部分（`storage.flush_interval_ms` 不在
/// 其中：它是 broker 的 delta 合并窗口，§6 第 10 条，存储层不参与）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    pub attachment_dir: PathBuf,
    /// §7.1 的 N 个只读连接。
    pub read_connections: u32,
    /// `storage.transcript_retention_days`（90）。
    pub transcript_retention_days: u32,
    /// `storage.sync_event_retention_days`（7）。
    pub sync_event_retention_days: u32,
    /// `storage.audit_retention_days`（365）。
    pub audit_retention_days: u32,
    /// `storage.max_total_size_bytes`（2 GiB）。
    pub max_total_size_bytes: u64,
    /// `storage.max_session_size_bytes`（100 MiB）。
    pub max_session_size_bytes: u64,
    /// `storage.persist_deltas`（false）。
    pub persist_deltas: bool,
    /// `storage.attachment_max_file_bytes`（20 MiB）。
    pub attachment_max_file_bytes: u64,
    /// `storage.attachment_max_total_bytes`（1 GiB）。
    pub attachment_max_total_bytes: u64,
    /// §7.1：正式模式下的权限检查失败关闭。
    pub strict_permissions: bool,
}

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

impl StorageConfig {
    /// 以 `<data_dir>` 为根，其余键取 §7.5 的默认值。
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        let attachment_dir = data_dir.join(ATTACHMENTS_DIR);
        StorageConfig {
            data_dir,
            attachment_dir,
            read_connections: DEFAULT_READ_CONNECTIONS,
            transcript_retention_days: 90,
            sync_event_retention_days: 7,
            audit_retention_days: 365,
            max_total_size_bytes: 2 * GIB,
            max_session_size_bytes: 100 * MIB,
            persist_deltas: false,
            attachment_max_file_bytes: 20 * MIB,
            attachment_max_total_bytes: GIB,
            strict_permissions: true,
        }
    }

    /// §7.1：单文件数据库路径。
    pub fn database_path(&self) -> PathBuf {
        self.data_dir.join(DATABASE_FILE)
    }
}

/// migration 之后 `meta` 中可用的库级事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreMetadata {
    /// §3.2 `ServerEpoch`：事件库首次创建时生成，升级与重启保持不变。
    pub server_epoch: String,
    pub owned_schema_version: i64,
    pub imported_schema_version: i64,
    pub created_at: String,
    pub last_prune_at: String,
}

/// 一次 `open` 的两个连接池：1 个写连接（`max_connections = 1`，串行化写事务）＋ N 个只读连接。
#[derive(Debug, Clone)]
pub struct Pools {
    pub write: SqlitePool,
    pub read: SqlitePool,
}

impl Pools {
    pub async fn close(self) {
        self.read.close().await;
        self.write.close().await;
    }
}

/// §7.1：打开连接池。目录与文件权限由本函数一并落实；`strict_permissions` 决定宽松权限是否失败关闭。
pub async fn open_pools(config: &StorageConfig) -> Result<Pools, StorageError> {
    prepare_data_dir(config)?;
    let write = open_write_pool(config).await?;
    tighten_database_files(config)?;
    let read = open_read_pool(config).await?;
    Ok(Pools { write, read })
}

/// §7.1：数据库/WAL/`-shm` 目标模式 `0600`。SQLite 按 umask 创建这些文件，所以打开后主动收紧。
fn tighten_database_files(config: &StorageConfig) -> Result<(), StorageError> {
    let base = config.database_path();
    let base = base.as_os_str().to_string_lossy().into_owned();
    for suffix in ["", "-wal", "-shm", "-journal"] {
        tighten_file_permissions(Path::new(&format!("{base}{suffix}")))?;
    }
    Ok(())
}

async fn open_write_pool(config: &StorageConfig) -> Result<SqlitePool, StorageError> {
    let options = SqliteConnectOptions::new()
        .filename(config.database_path())
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))
        .pragma("wal_autocheckpoint", WAL_AUTOCHECKPOINT_PAGES.to_string());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    Ok(pool)
}

async fn open_read_pool(config: &StorageConfig) -> Result<SqlitePool, StorageError> {
    // 只读连接不设 `journal_mode`（SQLite 拒绝在只读句柄上切换日志模式），也不 `create_if_missing`。
    let options = SqliteConnectOptions::new()
        .filename(config.database_path())
        .read_only(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS));
    let pool = SqlitePoolOptions::new()
        .max_connections(config.read_connections)
        .connect_with(options)
        .await?;
    Ok(pool)
}

/// §7.1：数据目录与附件目录按 `0700` 创建；Unix 上同时收紧已存在文件的模式。
fn prepare_data_dir(config: &StorageConfig) -> Result<(), StorageError> {
    create_dir(&config.data_dir)?;
    create_dir(&config.attachment_dir)?;
    for path in [&config.data_dir, &config.attachment_dir] {
        if config.strict_permissions {
            enforce_path_permissions(path)?;
        }
    }
    Ok(())
}

fn create_dir(path: &Path) -> Result<(), StorageError> {
    if path.is_dir() {
        return Ok(());
    }
    if path.exists() {
        return Err(StorageError::PathUnusable {
            path: path.display().to_string(),
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// §7.1：数据库/WAL/`-shm` 与附件文件的目标模式是 `0600`。
#[cfg(unix)]
pub fn tighten_file_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    if !path.exists() {
        return Ok(());
    }
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub fn tighten_file_permissions(_path: &Path) -> Result<(), StorageError> {
    // Windows：文件继承其所在目录的 ACL；§13.2 要求的「当前用户专属 ACL」由 §7.1 的目录检查兜底。
    Ok(())
}

/// 合成权限视图（§9.12）：平台 ACL 读取被抽象成纯函数，判定可单测。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PermissionView {
    pub group_read: bool,
    pub group_write: bool,
    pub other_read: bool,
    pub other_write: bool,
    /// 存在非当前用户的显式 ACL 条目（Windows ACL 视图）。
    pub foreign_principal_read: bool,
    pub foreign_principal_write: bool,
}

impl PermissionView {
    /// §9.12：宽松判定。目标状态是「只允许当前 OS 用户访问」（§13.2），因此除所有者位以外的任何
    /// 读/写授予都算宽松。
    pub fn is_relaxed(self) -> bool {
        self.group_read
            || self.group_write
            || self.other_read
            || self.other_write
            || self.foreign_principal_read
            || self.foreign_principal_write
    }

    /// Unix 模式位 → 视图：`group`/`other` 的读或写位都算宽松。
    pub fn from_unix_mode(mode: u32) -> Self {
        PermissionView {
            group_read: mode & 0o040 != 0,
            group_write: mode & 0o020 != 0,
            other_read: mode & 0o004 != 0,
            other_write: mode & 0o002 != 0,
            foreign_principal_read: false,
            foreign_principal_write: false,
        }
    }
}

/// 一次权限检查的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionCheck {
    /// 证据表明只有所有者可访问。
    OwnerOnly,
    /// 证据表明存在额外的读/写授予。
    Relaxed(PermissionView),
    /// 平台无法在不引入不安全代码的前提下读取 ACL。
    Unverifiable,
}

/// 读取路径的权限证据。
///
/// Windows 下读取 ACL 需要 `GetNamedSecurityInfoW` 一类的 API，而 workspace 禁止 `unsafe`
/// （`[workspace.lints.rust] unsafe_code = "forbid"`）且本项目未依赖任何 ACL 封装 crate，因此
/// Windows 返回 [`PermissionCheck::Unverifiable`]：正式模式不会因此失败关闭（§7.1 的失败关闭针对
/// **已经观察到**的宽松权限），数据库位于用户目录时其 ACL 由平台默认继承为当前用户专属。
#[cfg(unix)]
pub fn inspect_path_permissions(path: &Path) -> Result<PermissionCheck, StorageError> {
    use std::os::unix::fs::MetadataExt;
    let mode = std::fs::metadata(path)?.mode();
    let view = PermissionView::from_unix_mode(mode);
    Ok(if view.is_relaxed() {
        PermissionCheck::Relaxed(view)
    } else {
        PermissionCheck::OwnerOnly
    })
}

#[cfg(not(unix))]
pub fn inspect_path_permissions(_path: &Path) -> Result<PermissionCheck, StorageError> {
    Ok(PermissionCheck::Unverifiable)
}

/// §7.1/§9.12：正式模式下宽松即失败关闭。
pub fn enforce_path_permissions(path: &Path) -> Result<(), StorageError> {
    match inspect_path_permissions(path)? {
        PermissionCheck::Relaxed(_) => Err(StorageError::InsecurePermissions {
            path: path.display().to_string(),
        }),
        PermissionCheck::OwnerOnly | PermissionCheck::Unverifiable => Ok(()),
    }
}

/// §7.1：`PRAGMA quick_check`；失败即进入只读失败关闭。
pub async fn quick_check(pool: &SqlitePool) -> Result<(), StorageError> {
    let results: Vec<String> = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_all(pool)
        .await?;
    expect_ok(results)
}

/// 完整一致性检查；崩溃恢复判据用它（§9.2c）。
pub async fn integrity_check(pool: &SqlitePool) -> Result<(), StorageError> {
    let results: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_all(pool)
        .await?;
    expect_ok(results)
}

fn expect_ok(results: Vec<String>) -> Result<(), StorageError> {
    if results.len() == 1 && results.first().is_some_and(|row| row == "ok") {
        return Ok(());
    }
    // §8：错误消息不得包含正文。`quick_check`/`integrity_check` 的输出是页/索引结构描述，不含行内容，
    // 这里只保留固定文案。
    Err(StorageError::Corrupt(
        "sqlite integrity check did not return ok",
    ))
}

/// §7.2：执行 migration。单事务、幂等、失败整体回滚、版本过新拒绝启动。
///
/// 单事务的边界是 `BEGIN IMMEDIATE`：写池只有 1 个连接，但 `IMMEDIATE` 让「读版本 → 建表 → 写版本」
/// 在拿锁后整体执行，避免与外部进程的写事务在升级锁时冲突。
pub async fn migrate(write: &SqlitePool, at: &str) -> Result<StoreMetadata, StorageError> {
    let mut tx = write.begin_with("BEGIN IMMEDIATE").await?;

    let file_version = read_user_version(&mut *tx).await?;
    if file_version > FILE_FORMAT_VERSION {
        // 拒绝启动，且事务内没有任何写入（§7.2、§9.1）。
        return Err(StorageError::FileFormatTooNew {
            found: file_version,
            supported: FILE_FORMAT_VERSION,
        });
    }

    sqlx::raw_sql(OWNED_SCHEMA_V1).execute(&mut *tx).await?;
    sqlx::raw_sql(IMPORTED_SCHEMA_V1).execute(&mut *tx).await?;

    let owned_version = read_meta_row(&mut *tx, META_OWNED_SCHEMA_VERSION)
        .await?
        .map(|value| parse_version(&value, "meta.owned_schema_version"))
        .transpose()?
        .unwrap_or(OWNED_SCHEMA_VERSION);
    let imported_version = read_meta_row(&mut *tx, META_IMPORTED_SCHEMA_VERSION)
        .await?
        .map(|value| parse_version(&value, "meta.imported_schema_version"))
        .transpose()?
        .unwrap_or(IMPORTED_SCHEMA_VERSION);

    if owned_version > OWNED_SCHEMA_VERSION || imported_version > IMPORTED_SCHEMA_VERSION {
        return Err(StorageError::SchemaTooNew {
            found: owned_version.max(imported_version),
            supported: OWNED_SCHEMA_VERSION.min(IMPORTED_SCHEMA_VERSION),
        });
    }

    write_meta_if_absent(&mut tx, META_SERVER_EPOCH, &new_server_epoch()).await?;
    write_meta_if_absent(&mut tx, META_CREATED_AT, at).await?;
    write_meta_if_absent(&mut tx, META_LAST_PRUNE_AT, at).await?;
    write_meta_if_absent(
        &mut tx,
        META_OWNED_SCHEMA_VERSION,
        &OWNED_SCHEMA_VERSION.to_string(),
    )
    .await?;
    write_meta_if_absent(
        &mut tx,
        META_IMPORTED_SCHEMA_VERSION,
        &IMPORTED_SCHEMA_VERSION.to_string(),
    )
    .await?;

    if file_version != FILE_FORMAT_VERSION {
        tx.execute(format!("PRAGMA user_version = {FILE_FORMAT_VERSION}").as_str())
            .await?;
    }

    let server_epoch = read_meta_row(&mut *tx, META_SERVER_EPOCH)
        .await?
        .ok_or(StorageError::InvalidRequest("meta.server_epoch missing"))?;
    let created_at = read_meta_row(&mut *tx, META_CREATED_AT)
        .await?
        .ok_or(StorageError::InvalidRequest("meta.created_at missing"))?;
    let last_prune_at = read_meta_row(&mut *tx, META_LAST_PRUNE_AT)
        .await?
        .ok_or(StorageError::InvalidRequest("meta.last_prune_at missing"))?;

    tx.commit().await?;

    Ok(StoreMetadata {
        server_epoch,
        owned_schema_version: OWNED_SCHEMA_VERSION,
        imported_schema_version: IMPORTED_SCHEMA_VERSION,
        created_at,
        last_prune_at,
    })
}

/// §3.2：`ServerEpoch` 首次创建时生成，随后保持不变。用 UUID v4（随机源，不读系统时间）。
fn new_server_epoch() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn read_user_version<'e, E>(executor: E) -> Result<i64, StorageError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(executor)
        .await?;
    Ok(version)
}

/// 读取 `meta` 中的一行。表不存在时返回 `None`（首次创建）。
pub async fn read_meta_row<'e, E>(executor: E, key: &str) -> Result<Option<String>, StorageError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
        .bind(key)
        .fetch_optional(executor)
        .await?;
    Ok(row.map(|(value,)| value))
}

/// 只在键不存在时写入：保证第二次启动不产生任何行级写入（§9.1 的逐字节幂等）。
pub async fn write_meta_if_absent(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    key: &str,
    value: &str,
) -> Result<(), StorageError> {
    sqlx::query("INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO NOTHING")
        .bind(key)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// `meta` 的版本值必须是十进制非负整数；其它取值说明库被外部改写。
fn parse_version(value: &str, column: &'static str) -> Result<i64, StorageError> {
    value.parse::<i64>().map_err(|_| StorageError::ColumnValue {
        column,
        expected: "decimal integer",
    })
}

/// §7.1：`PRAGMA wal_checkpoint(TRUNCATE)`。失败不影响调用结果——检查点只是把 WAL 归并回主库，
/// 未归并的 WAL 在下次打开时仍会被正常读取。
pub async fn checkpoint_truncate(pool: &SqlitePool) {
    let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await;
}

/// §7.5 的保留窗口算术。
///
/// `Timestamp` 在 core 里是不透明文本（固定 `%Y-%m-%dT%H:%M:%S%.3fZ`，§3.2），core 也不暴露任何
/// 日期算法；本 crate 不能读系统时间，也没有日期库依赖，因此这里只做**纯函数**的公历换算：
/// 解析 → 加减整天 → 按同一格式重新格式化。闰年与月长由 Hinnant 的 `days_from_civil`/
/// `civil_from_days` 覆盖，不涉及时区（时间戳恒为 UTC 且带 `Z`）。
pub mod window {
    /// 一天的毫秒数。
    pub const DAY_MILLIS: i64 = 86_400_000;

    /// 解析 `YYYY-MM-DDTHH:MM:SS.mmmZ`，返回 Unix 毫秒。形状非法（长度、分隔符、字段范围）返回
    /// `None`——调用方的 `Timestamp` 已经过 core 校验，这里只防御被外部改写的库。
    pub fn parse_millis(text: &str) -> Option<i64> {
        let bytes = text.as_bytes();
        if bytes.len() != 24 {
            return None;
        }
        if bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes[10] != b'T'
            || bytes[13] != b':'
            || bytes[16] != b':'
            || bytes[19] != b'.'
            || bytes[23] != b'Z'
        {
            return None;
        }
        let field = |from: usize, to: usize| -> Option<i64> {
            let slice = text.get(from..to)?;
            if !slice.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            slice.parse::<i64>().ok()
        };
        let year = field(0, 4)?;
        let month = field(5, 7)?;
        let day = field(8, 10)?;
        let hour = field(11, 13)?;
        let minute = field(14, 16)?;
        let second = field(17, 19)?;
        let millis = field(20, 23)?;
        if !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || !(0..=23).contains(&hour)
            || !(0..=59).contains(&minute)
            || !(0..=59).contains(&second)
        {
            return None;
        }
        // 日历有效性（闰日、月长）**刻意不校验**：core 的 `Timestamp` 也只校验字段范围
        // （`model/scalars.rs` 的 `is_timestamp`），这里若更严格就会拒绝 core 已接受的值。
        let days = days_from_civil(year, month as u32, day as u32);
        days.checked_mul(DAY_MILLIS)?
            .checked_add(hour * 3_600_000 + minute * 60_000 + second * 1_000 + millis)
    }

    /// 按 `YYYY-MM-DDTHH:MM:SS.mmmZ` 格式化 Unix 毫秒。
    pub fn format_millis(millis: i64) -> String {
        let days = millis.div_euclid(DAY_MILLIS);
        let rest = millis.rem_euclid(DAY_MILLIS);
        let (year, month, day) = civil_from_days(days);
        let hour = rest / 3_600_000;
        let minute = rest % 3_600_000 / 60_000;
        let second = rest % 60_000 / 1_000;
        let milli = rest % 1_000;
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z")
    }

    /// `text + days` 天；`days` 为负即减法。
    pub fn shift_days(text: &str, days: i64) -> Option<String> {
        let millis = parse_millis(text)?;
        let shifted = millis.checked_add(days.checked_mul(DAY_MILLIS)?)?;
        Some(format_millis(shifted))
    }

    /// Howard Hinnant, `days_from_civil`：`1970-01-01` 为 0。
    fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
        let year = if month <= 2 { year - 1 } else { year };
        let era = if year >= 0 { year } else { year - 399 } / 400;
        let yoe = year - era * 400;
        let mp = ((month + 9) % 12) as i64;
        let doy = (153 * mp + 2) / 5 + day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Howard Hinnant, `civil_from_days`。
    fn civil_from_days(days: i64) -> (i64, u32, u32) {
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if month <= 2 { year + 1 } else { year };
        (year, month as u32, day as u32)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_and_formats_round_trip() {
            for text in [
                "2026-09-18T00:00:00.000Z",
                "2024-02-29T23:59:59.999Z",
                "2000-02-29T12:00:00.000Z",
                "1970-01-01T00:00:00.000Z",
            ] {
                let millis = parse_millis(text).expect("parses");
                assert_eq!(format_millis(millis), text);
            }
        }

        #[test]
        fn rejects_malformed_shapes() {
            for text in [
                "",
                "2026-09-18T00:00:00.000",
                "2026-09-18 00:00:00.000Z",
                "2026-13-18T00:00:00.000Z",
                "2026-09-32T00:00:00.000Z",
                "2026-09-18T24:00:00.000Z",
                "2026-09-18T00:00:60.001Z",
                "2026-0a-18T00:00:00.000Z",
            ] {
                assert!(parse_millis(text).is_none(), "{text} must be rejected");
            }
        }

        #[test]
        fn shifts_across_month_and_year_boundaries() {
            assert_eq!(
                shift_days("2026-09-18T00:00:00.000Z", 90).as_deref(),
                Some("2026-12-17T00:00:00.000Z")
            );
            assert_eq!(
                shift_days("2026-12-31T23:59:59.999Z", 1).as_deref(),
                Some("2027-01-01T23:59:59.999Z")
            );
            assert_eq!(
                shift_days("2024-02-28T00:00:00.000Z", 1).as_deref(),
                Some("2024-02-29T00:00:00.000Z")
            );
            assert_eq!(
                shift_days("2026-09-18T00:00:00.000Z", -90).as_deref(),
                Some("2026-06-20T00:00:00.000Z")
            );
        }
    }
}
