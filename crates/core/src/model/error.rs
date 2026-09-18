//! `docs/CORE_PORTS_AND_STORAGE.md` §2 的统一错误类型，以及 §3 值对象在**构造期**使用的具名校验错误。
//!
//! `PortError` 逐字照 §2；本文件的其余类型是 model 层为了“非法输入返回具名错误”而定义的自有错误域：
//! 每个 newtype/记录构造器返回 `Result<_, InvalidValue>`，并通过 `From<InvalidValue> for PortError`
//! 在端口边界收敛成 `PortError::InvalidRequest`。这样适配器既能对具体拒绝原因做分支，也不必在 core
//! 里引入第二套错误层级。

use std::convert::Infallible;
use std::fmt;

use super::EntityRef;

/// 冲突原因（§2）。取值与 Sync / Node Link 的错误码一一对应，且只描述 core 判断出的冲突类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConflictKind {
    /// `state.version_conflict`：`expectedVersion` 与当前会话版本不一致。
    VersionMismatch,
    /// `interaction.already_resolved`：first-writer-wins 已有结论。
    AlreadyResolved,
    /// `pairing.already_claimed`：pairing 已被另一个 claim 占用。
    AlreadyClaimed,
    /// `pairing.expired`：pairing 超过 `expiresAt`。
    Expired,
    /// `pairing.consumed`：pairing 已被首次 WSS 认证消耗。
    Consumed,
    /// `command.idempotency_conflict`：同一 `(actor, requestId)` 携带了不同的命令语义。
    IdempotencyConflict,
}

impl ConflictKind {
    /// 全部取值，顺序即声明顺序（供存储层与测试穷举，不参与 wire 编码）。
    pub const ALL: [Self; 6] = [
        Self::VersionMismatch,
        Self::AlreadyResolved,
        Self::AlreadyClaimed,
        Self::Expired,
        Self::Consumed,
        Self::IdempotencyConflict,
    ];

    /// 稳定的下划线标记，用于日志与持久层分支，不是 wire 错误码。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VersionMismatch => "version_mismatch",
            Self::AlreadyResolved => "already_resolved",
            Self::AlreadyClaimed => "already_claimed",
            Self::Expired => "expired",
            Self::Consumed => "consumed",
            Self::IdempotencyConflict => "idempotency_conflict",
        }
    }
}

impl fmt::Display for ConflictKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 临时不可用原因（§2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnavailableKind {
    /// 会话 actor 忙（`sessions.queue_policy = reject_busy` 或队列已满）。
    Busy,
    /// 容量上限：`storage.max_total_size_bytes` / `max_session_size_bytes`。
    StorageFull,
    /// 磁盘或文件系统 I/O 失败。
    IoError,
    /// imported 资源在线回源时 Owner 不可达。
    RemoteUnavailable,
    /// Owner Node 离线（Node Link usage）。
    OwnerOffline,
    /// Export 已撤销。
    ExportRevoked,
}

impl UnavailableKind {
    /// 全部取值，顺序即声明顺序。
    pub const ALL: [Self; 6] = [
        Self::Busy,
        Self::StorageFull,
        Self::IoError,
        Self::RemoteUnavailable,
        Self::OwnerOffline,
        Self::ExportRevoked,
    ];

    /// 稳定的下划线标记（同上，不是 wire 错误码）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::StorageFull => "storage_full",
            Self::IoError => "io_error",
            Self::RemoteUnavailable => "remote_unavailable",
            Self::OwnerOffline => "owner_offline",
            Self::ExportRevoked => "export_revoked",
        }
    }
}

impl fmt::Display for UnavailableKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 端口统一错误（`docs/CORE_PORTS_AND_STORAGE.md` §2，逐字照搬）。
///
/// 适配器错误（`sqlx::Error` 等）必须先在适配器内映射成本枚举之一才可进入 core
/// （`docs/MODULE_ARCHITECTURE.md` §8）。错误消息不得包含 SQL 文本、完整 prompt、密钥或 ACP raw（§8）。
#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("not found: {0}")]
    NotFound(EntityRef),
    #[error("conflict: {0}")]
    Conflict(ConflictKind),
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),
    #[error("unavailable: {0}")]
    Unavailable(UnavailableKind),
    #[error("corrupt: {0}")]
    Corrupt(&'static str),
    #[error("backend failure: {0}")]
    Backend(Box<dyn std::error::Error + Send + Sync>),
}

impl From<InvalidValue> for PortError {
    fn from(value: InvalidValue) -> Self {
        Self::InvalidRequest(value.as_str())
    }
}

/// `TryFrom<u64>` 等不会失败的转换路径需要把 `Infallible` 收敛进 `PortError`。
impl From<Infallible> for PortError {
    fn from(never: Infallible) -> Self {
        match never {}
    }
}

/// §3 值对象的具名构造错误。
///
/// 与 `PortError` 的分工：构造器只表达“这个值非法”，由调用方决定是 `InvalidRequest` 还是别的语义。
/// `Copy` 且可比较，便于在测试里逐条断言拒绝原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvalidValue {
    /// 不是 canonical 小写 UUID 文本。
    Uuid,
    /// `ExportId` 不匹配 `^[A-Za-z0-9._-]{1,128}$`。
    ExportId,
    /// `ImportId` 不匹配 `^[A-Za-z0-9._-]{1,128}$`。
    ImportId,
    /// `WorkspaceAlias` 不匹配 `^[a-z0-9][a-z0-9._-]{0,63}$`。
    WorkspaceAlias,
    /// `TemplateId` 不匹配 `^[A-Za-z0-9._-]{1,64}$`。
    TemplateId,
    /// `AgentId` 为空或超过 128 个字符。
    AgentId,
    /// `EventType` 不匹配 `^[a-z0-9_.-]{1,128}$`。
    EventType,
    /// scope 名不匹配 `^[a-z][a-z0-9._-]{0,127}$`。
    ScopeName,
    /// grant 名不匹配 `^grant\.[a-z][a-z0-9._-]{0,63}$`。
    GrantName,
    /// `PublicError.code` 不匹配 `^[a-z][a-z0-9._-]{0,127}$`。
    ErrorCode,
    /// `Sequence` 超过 `2^63-1`。
    SequenceRange,
    /// 不是 32 字节 SHA-256 的规范无填充 base64url 文本。
    Digest,
    /// 不是 32 字节随机数的规范无填充 base64url 文本。
    Nonce,
    /// 不是 `%Y-%m-%dT%H:%M:%S%.3fZ`，或字段越界。
    Timestamp,
    /// 不是 64 字符小写十六进制指纹。
    Fingerprint,
    /// 不是非空 `wss://` 端点。
    Endpoint,
    /// 不是良构 JSON object（`ViewJson`）。
    Json,
    /// 不是良构 JSON document（ACP raw）。
    JsonDocument,
    /// JSON 嵌套深度超过上限。
    Depth { max: u16 },
    /// 字节数超过上限。
    TooLarge { max: usize },
    /// 字符数超过上限。
    TooLong { max: usize },
    /// 不允许为空。
    Empty,
    /// 违反跨字段不变量（如 `closed_at` 与 `state = closed` 不一致）。
    Field,
    /// `PersistencePolicy::Ephemeral` 不可进入任何提交（§3.4）。
    Ephemeral,
    /// `ElicitationAction::Cancel` 时 `ElicitationValues` 必须为 `None`（§3.3 `[待确认]` 已冻结部分）。
    ElicitationValues,
    /// 状态机不允许该转换，或源状态已是终态。
    StateTransition,
}

impl InvalidValue {
    /// 稳定的静态消息，供 `PortError::InvalidRequest(&'static str)` 使用。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uuid => "id is not a canonical lowercase UUID",
            Self::ExportId => "export id must match ^[A-Za-z0-9._-]{1,128}$",
            Self::ImportId => "import id must match ^[A-Za-z0-9._-]{1,128}$",
            Self::WorkspaceAlias => "workspace alias must match ^[a-z0-9][a-z0-9._-]{0,63}$",
            Self::TemplateId => "template id must match ^[A-Za-z0-9._-]{1,64}$",
            Self::AgentId => "agent id must be 1..=128 characters",
            Self::EventType => "event type must match ^[a-z0-9_.-]{1,128}$",
            Self::ScopeName => "scope name must match ^[a-z][a-z0-9._-]{0,127}$",
            Self::GrantName => "grant name must match ^grant\\.[a-z][a-z0-9._-]{0,63}$",
            Self::ErrorCode => "code must match ^[a-z][a-z0-9._-]{0,127}$",
            Self::SequenceRange => "sequence exceeds 2^63-1",
            Self::Digest => "digest must be 32 bytes of canonical unpadded base64url",
            Self::Nonce => "nonce must be 32 bytes of canonical unpadded base64url",
            Self::Timestamp => "timestamp must be YYYY-MM-DDTHH:MM:SS.mmmZ with in-range fields",
            Self::Fingerprint => "fingerprint must be 64 lowercase hex characters",
            Self::Endpoint => "endpoint must be a non-empty wss:// URL",
            Self::Json => "value must be a well-formed JSON object",
            Self::JsonDocument => "value must be a well-formed JSON document",
            Self::Depth { .. } => "json nesting depth exceeds the allowed maximum",
            Self::TooLarge { .. } => "value exceeds the maximum byte size",
            Self::TooLong { .. } => "value exceeds the maximum length",
            Self::Empty => "value must not be empty",
            Self::Field => "value violates a cross-field invariant",
            Self::Ephemeral => "ephemeral events cannot be persisted",
            Self::ElicitationValues => {
                "elicitation values must be absent when the action is cancel"
            }
            Self::StateTransition => "state transition is not allowed",
        }
    }
}

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Depth { max } => write!(f, "json nesting depth exceeds {max}"),
            Self::TooLarge { max } => write!(f, "value exceeds {max} bytes"),
            Self::TooLong { max } => write!(f, "value exceeds {max} characters"),
            other => f.write_str(other.as_str()),
        }
    }
}

impl std::error::Error for InvalidValue {}
