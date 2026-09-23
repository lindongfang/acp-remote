//! 适配器错误与 `core::ports::PortError` 的映射边界。
//!
//! `docs/MODULE_ARCHITECTURE.md` §8：`sqlx::Error` 不得进入 core，适配器必须在边界映射成
//! `PortError`；`docs/CORE_PORTS_AND_STORAGE.md` §8 还要求错误消息不得包含 SQL 文本、完整 prompt、
//! 密钥或 ACP raw。因此进入 `PortError::Backend` 的载荷是 [`SqlFailure`]——它只保留 SQLite 的结果码
//! 与截断清洗后的消息（SQLite 的约束消息只含表名/列名，不含语句文本、也不含绑定值），不保留原始
//! `sqlx::Error` 对象。
//!
//! 本模块是 `storage-sqlite` 与 core 之间的**唯一**错误转换点：`?` 在本 crate 内把 `sqlx::Error`
//! 收成 [`StorageError`]，出口处 [`StorageError::into_port_error`] 把它变成 `PortError`。

use acp_core::model::{ConflictKind, PortError, UnavailableKind};

/// `storage-sqlite` 的内部错误。
///
/// 变体只表达**本适配器**能判定的失败；`sqlx` 的细节由 [`SqlFailure`] 承载。所有变体都必须能映射成
/// `PortError`（[`StorageError::into_port_error`]），且不得携带 SQL 文本或正文。
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("sqlite failure: {0}")]
    Sql(#[from] SqlFailure),
    #[error("filesystem failure: {0}")]
    Io(#[source] std::io::Error),
    /// 数据库文件格式版本高于本二进制已知版本（§7.2）：拒绝启动，且不得写入任何行。
    #[error("database file format version {found} is newer than supported {supported}")]
    FileFormatTooNew { found: i64, supported: i64 },
    /// 某一族 schema 版本高于本二进制已知版本（§7.2）。
    #[error("schema version {found} is newer than supported {supported}")]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("path is not a usable directory: {path}")]
    PathUnusable { path: String },
    #[error("permissions on {path} are too permissive")]
    InsecurePermissions { path: String },
    /// §7.1：`quick_check` 失败后进入只读失败关闭，写路径一律返回本错误。
    #[error("store is read-only after failure: {0}")]
    FailClosed(&'static str),
    #[error("corrupt store: {0}")]
    Corrupt(&'static str),
    #[error("value out of range: {0}")]
    OutOfRange(&'static str),
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),
    /// §7.5：容量清理后仍超限时拒绝新写入，不静默丢弃。
    #[error("storage capacity exceeded")]
    StorageFull,
    /// 库中出现了本二进制无法解释的列值（判定为损坏而不是静默降级）。
    #[error("column {column} is not a valid {expected}")]
    ColumnValue {
        column: &'static str,
        expected: &'static str,
    },
}

impl StorageError {
    /// §11.6/§9.23：管理写集里的**约束失败**必须给出语义化冲突（唯一键/条件更新 → `AlreadyExists`、
    /// 身份材料不一致 → `IdentityMismatch`、归属冲突 → `DuplicateOwnership`），不能落进 `Backend`。
    /// `kind` 由调用点决定——只有调用点知道自己在写什么，因此映射不放进 [`Self::into_port_error`]
    /// （owned/imported 的既有路径语义不变，它们的约束失败仍由用例层判定）。
    pub fn into_conflict(self, kind: ConflictKind) -> PortError {
        match self {
            StorageError::Sql(failure) if failure.is_constraint() => PortError::Conflict(kind),
            other => other.into_port_error(),
        }
    }

    /// `docs/MODULE_ARCHITECTURE.md` §8：`sqlx::Error` 不得进入 core，适配器必须在边界映射。
    ///
    /// | `StorageError` | `PortError` |
    /// |---|---|
    /// | `Sql(BUSY/LOCKED)` | `Unavailable(Busy)` |
    /// | `Sql(FULL)` | `Unavailable(StorageFull)` |
    /// | `Sql(CORRUPT/NOTADB)` | `Corrupt` |
    /// | `Sql(other)` | `Backend`（清洗后的 [`SqlFailure`]） |
    /// | `Io` | `Unavailable(IoError)` |
    /// | `FileFormatTooNew`/`SchemaTooNew` | `InvalidRequest` |
    /// | `PathUnusable`/`InsecurePermissions` | `Unavailable(IoError)` |
    /// | `FailClosed`/`Corrupt`/`OutOfRange` | `Corrupt` |
    /// | `InvalidRequest` | `InvalidRequest` |
    /// | `StorageFull` | `Unavailable(StorageFull)` |
    /// | `ColumnValue` | `Backend` |
    pub fn into_port_error(self) -> PortError {
        match self {
            StorageError::Sql(failure) => failure.into_port_error(),
            StorageError::Io(_) => PortError::Unavailable(UnavailableKind::IoError),
            StorageError::FileFormatTooNew { .. } | StorageError::SchemaTooNew { .. } => {
                PortError::InvalidRequest("store schema is newer than this binary")
            }
            StorageError::PathUnusable { .. } | StorageError::InsecurePermissions { .. } => {
                PortError::Unavailable(UnavailableKind::IoError)
            }
            StorageError::FailClosed(reason) | StorageError::Corrupt(reason) => {
                PortError::Corrupt(reason)
            }
            StorageError::OutOfRange(what) => PortError::Corrupt(what),
            StorageError::InvalidRequest(what) => PortError::InvalidRequest(what),
            StorageError::StorageFull => PortError::Unavailable(UnavailableKind::StorageFull),
            StorageError::ColumnValue { column, expected } => {
                PortError::Backend(Box::new(ColumnCorruption { column, expected }))
            }
        }
    }
}

/// 端口实现里 `?` 直接把适配器错误收敛成 `PortError`（§8 的唯一出口）。
impl From<StorageError> for PortError {
    fn from(error: StorageError) -> Self {
        error.into_port_error()
    }
}

/// 具名校验错误（`InvalidValue`）与端口错误同源：都是「这个值非法」（§2）。
impl From<acp_core::model::InvalidValue> for StorageError {
    fn from(_: acp_core::model::InvalidValue) -> Self {
        StorageError::InvalidRequest("value does not satisfy its domain shape")
    }
}

impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        StorageError::Io(error)
    }
}

impl From<sqlx::Error> for StorageError {
    fn from(error: sqlx::Error) -> Self {
        StorageError::Sql(error.into())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("column {column} is not a valid {expected}")]
struct ColumnCorruption {
    column: &'static str,
    expected: &'static str,
}

/// 经过清洗的 SQLite 失败：保留结果码与截断后的消息，不保留语句文本与绑定值。
#[derive(Debug)]
pub struct SqlFailure {
    code: Option<String>,
    message: String,
}

// SQLite 主结果码（`sqlite3_errcode` 的低 8 位；扩展码的高位不参与判定）。
const SQLITE_BUSY: i32 = 5;
const SQLITE_LOCKED: i32 = 6;
const SQLITE_CORRUPT: i32 = 11;
const SQLITE_FULL: i32 = 13;
const SQLITE_CONSTRAINT: i32 = 19;
const SQLITE_NOTADB: i32 = 26;

impl SqlFailure {
    fn primary_code(&self) -> Option<i32> {
        let parsed: i32 = self.code.as_deref()?.parse().ok()?;
        Some(parsed & 0xff)
    }

    /// 是否由约束触发（`SQLITE_CONSTRAINT` 及其扩展码：唯一键、外键、CHECK、NOT NULL）。
    ///
    /// 管理写集用它把「唯一键/条件更新失败」翻成具名冲突（[`StorageError::into_conflict`]）；
    /// 通用映射（[`Self::into_port_error`]）不区分约束，保持既有路径的判定不变。
    pub fn is_constraint(&self) -> bool {
        self.primary_code() == Some(SQLITE_CONSTRAINT)
    }

    fn into_port_error(self) -> PortError {
        match self.primary_code() {
            Some(SQLITE_BUSY | SQLITE_LOCKED) => PortError::Unavailable(UnavailableKind::Busy),
            Some(SQLITE_FULL) => PortError::Unavailable(UnavailableKind::StorageFull),
            Some(SQLITE_CORRUPT | SQLITE_NOTADB) => PortError::Corrupt("sqlite reports corruption"),
            _ => PortError::Backend(Box::new(self)),
        }
    }
}

impl std::fmt::Display for SqlFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.code {
            Some(code) => write!(f, "sqlite code {code}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for SqlFailure {}

/// 消息清洗上限（字节）：错误消息只用于诊断，不承载正文。
const MESSAGE_LIMIT: usize = 200;

fn sanitize(message: &str) -> String {
    let mut out = String::with_capacity(message.len().min(MESSAGE_LIMIT));
    let mut pending_space = false;
    for ch in message.chars() {
        if out.len() >= MESSAGE_LIMIT {
            break;
        }
        if ch.is_control() || ch.is_whitespace() {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(ch);
        }
    }
    out
}

impl From<sqlx::Error> for SqlFailure {
    fn from(error: sqlx::Error) -> Self {
        let (code, message) = match &error {
            sqlx::Error::Database(db) => (
                db.code().map(|code| code.to_string()),
                sanitize(db.message()),
            ),
            _ => (None, sanitize(&error.to_string())),
        };
        SqlFailure { code, message }
    }
}
