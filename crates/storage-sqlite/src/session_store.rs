//! `SessionStore` / `ReadView` / `AttachmentStore` 的 SQLite 实现。
//!
//! 权威是 `docs/CORE_PORTS_AND_STORAGE.md`：§5.2 的端口形状、§6 的事务契约、§7 的表结构与 PRAGMA、
//! §7.5 的保留/清理/容量、§9 的验收判据。三条硬规则贯穿本文件：
//!
//! - **单写入口**：owned 家族只有 `SessionStore::commit` 会写 `owned_*`（状态 + turn + 事件 + 幂等记录 +
//!   命令终态在**同一事务**内）；没有可分别调用的 repository/journal/deduper
//!   （`MODULE_ARCHITECTURE.md` §4.1）。远程资源没有持久族：本 crate 不保存任何 Owner 正文、摘要、
//!   游标或命令引用。
//! - **不读系统时间**：所有时间来自参数（§2）；保留窗口的算术用 [`crate::migrate::window`] 的纯函数。
//!
//! 读列一律用显式列名（`row.try_get("列名")`），因此 Rust 侧字段顺序与表结构解耦；不用
//! `sqlx::FromRow` 派生是因为 workspace 的 `sqlx` features 没有 `macros`（不为此改依赖）。

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::PathBuf;
use std::str::FromStr;

use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::{Row, Sqlite, SqliteConnection, Transaction};
use tokio::sync::Mutex;

use acp_core::broker::HISTORY_ENTRY_EVENT_TYPES;
use acp_core::model::{
    AcpRaw, Actor, ActorKind, AgentId, AgentRef, AgentSessionId, AttachmentGeneration,
    AttachmentId, CommandRecord, CommandResult, CommandStatus, CommittedEvent, ConflictKind,
    DeviceId, Digest, DirectoryId, EntityRef, EventId, EventKind, EventOrigin, EventPayload,
    GlobalCursor, InteractionId, InteractionOption, InteractionResolution, MediaKind,
    MediaReference, MessageChannel, MessageId, MessageRecord, MessageState, ModeId, ModeRef,
    NodeId, OriginCursor, OriginEpoch, PartialReason, PartialResult, PendingInteraction, PortError,
    PublicError, RawUnavailableReason, RequestId, ResourceOrigin, Sequence, ServerEpoch, Session,
    SessionId, SessionRecoveryRecord, SessionSnapshot, SessionState, SessionSummary, StoredPolicy,
    Timestamp, Turn, TurnId, UnavailableKind, Version, ViewJson, WorkspaceRef,
};
use acp_core::ports::{
    AttachmentRef, AttachmentStore, CommitOutcome, HistoryPage, HistoryQuery, IdempotentReplay,
    ModeChange, NewTurn, NodeLinkSlice, OwnedCommit, OwnedEventRecord, PendingInteractionOrigin,
    PruneReport, ReadView, ReplayBatch, ReplayLimit, ResetReason, RetentionPolicy,
    SessionActivityFacts, SessionQuery, SessionStore, SettledSession, StateChange, StoreHealth,
    SubmissionBinding, SyncEventRecord, SyncReplaySlice, TurnChange, TurnUpdate,
};

use crate::error::StorageError;
use crate::migrate::{self, Pools, StorageConfig, StoreMetadata, window};

/// `owned_event` 的读列（顺序无关，集中一处便于与 §7.3 对照）。
const EVENT_COLUMNS: &str = "global_sequence, session_id, session_sequence, origin_epoch, \
     origin_sequence, event_id, turn_id, event_type, kind, policy, origin_kind, causation, \
     payload_json, payload_digest, acp_media_type, acp_raw_json, acp_byte_length, acp_sha256, \
     acp_raw_unavailable_reason, created_at, expires_at, compacted_into";

/// `owned_session` 的读列（`load` 的 `Session` 聚合：不带目录归属）。
const SESSION_COLUMNS: &str = "session_id, title, agent_id, agent_name, state, origin_epoch, \
     current_mode_id, current_mode_name, version, created_at, updated_at";

/// 返回 [`SessionSummary`] 的三条读路径（`list`/`read_session`/`node_link_slice`）的读列：
/// `SESSION_COLUMNS` 加上**持久化的目录标识**。
///
/// 归属的唯一来源是 `owned_session.workspace_directory_id`；`MUST NOT` 按 `workspace_cwd` 反查目录表
/// ——`NULL` 就是未分组（`design.md` D1/D6）。
const SESSION_SUMMARY_COLUMNS: &str = "s.session_id, s.title, s.agent_id, s.agent_name, s.state, \
     s.origin_epoch, s.current_mode_id, s.current_mode_name, s.version, s.created_at, \
     s.updated_at, s.workspace_directory_id";

/// [`SESSION_SUMMARY_COLUMNS`] 的 FROM 子句。
const SESSION_SUMMARY_FROM: &str = "owned_session s";

/// `owned_command` 的读列。
const COMMAND_COLUMNS: &str = "actor_kind, actor_id, request_id, session_id, command, kind, \
     expected_version, request_fingerprint, accepted_at, status, terminal_at, terminal_event_id, \
     result_json, error_code, error_message, error_details_json, retryable";

// ---------------------------------------------------------------------------------------------
// 存储
// ---------------------------------------------------------------------------------------------

/// SQLite 持久化后端（§7）。
///
/// 连接模型是 §7.1 的「1 写 + N 读」：`write` 池 `max_connections = 1`，写事务因此天然串行；
/// `read` 池只读，`read_view()` 在它上面开显式只读事务。
pub struct SqliteStore {
    pools: Pools,
    config: StorageConfig,
    meta: StoreMetadata,
    /// §7.5 的容量度量语句：`open` 时从 schema 现读 TEXT 列拼出（列增删不可能漏），之后每次检量只跑它。
    measure_sql: String,
    /// §7.1：启动 `quick_check` 的结论；`false` 即只读失败关闭（写路径返回 `PortError::Corrupt`）。
    integrity_ok: bool,
}

impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStore")
            .field("data_dir", &self.config.data_dir)
            .field("integrity_ok", &self.integrity_ok)
            .finish()
    }
}

impl SqliteStore {
    /// 写池（管理 store 模块共用）：§7.1 的单写连接，管理写集在它上面开 BEGIN IMMEDIATE。
    pub(crate) fn pools(&self) -> &Pools {
        &self.pools
    }

    /// 附件/传输实现共用的存储配置（附件目录、单附件上限等）。
    pub(crate) fn config(&self) -> &StorageConfig {
        &self.config
    }
    /// v10 升级时被删除的旧 Agent profile 行数（用于启动提示；没有迁移记录即 0）。
    pub async fn discarded_agent_profiles(&self) -> Result<u64, StorageError> {
        let Some(value) =
            migrate::read_meta_row(&self.pools.write, migrate::META_DISCARDED_AGENT_PROFILES)
                .await?
        else {
            return Ok(0);
        };
        value
            .parse()
            .map_err(|_| StorageError::Corrupt("discarded_agent_profiles"))
    }

    /// §7.1/§7.2：打开（必要时创建）数据库、设置 PRAGMA、跑 `quick_check` 与 migration。
    ///
    /// `at` 由调用方从 `Clock` 取得（§2：本 crate 不读系统时间）：它只用于首次创建库时的
    /// `meta.created_at`/`meta.last_prune_at`。`meta.server_epoch` 首次创建时生成，此后保持不变。
    ///
    /// `quick_check` 失败时不返回错误而是进入**只读失败关闭**（§7.1）：只读查询继续，写路径返回
    /// `PortError::Corrupt`。若连 `meta` 都读不出来（库文件不可用），则打开失败。
    pub async fn open(config: StorageConfig, at: &Timestamp) -> Result<Self, StorageError> {
        let pools = migrate::open_pools(&config).await?;
        let integrity_ok = match migrate::quick_check(&pools.write).await {
            Ok(()) => true,
            Err(failure) => {
                // 失败的 quick_check 也可能是「文件根本不是数据库」：那时连 meta 都读不出来。
                if migrate::read_meta_row(&pools.write, migrate::META_SERVER_EPOCH)
                    .await?
                    .is_none()
                {
                    return Err(failure);
                }
                false
            }
        };
        let meta = if integrity_ok {
            migrate::migrate(&pools.write, at.as_str()).await?
        } else {
            read_metadata(&pools.write).await?
        };
        let mut write = pools.write.acquire().await.db()?;
        let measure_sql = build_measure_sql(&mut write).await?;
        drop(write);
        Ok(SqliteStore {
            pools,
            config,
            meta,
            measure_sql,
            integrity_ok,
        })
    }

    /// §7.2 的启动清理门禁（`access-owner-data-stateless`）：迁移成功之后、开始监听之前调用一次。
    ///
    /// 1. `wal_checkpoint(TRUNCATE)`：把迁移写入的 WAL 归并回主库，并清空 WAL 文件；
    /// 2. `secure_delete = ON` + `VACUUM`：重写数据库、丢弃自由页里被删除的旧页副本；
    /// 3. 写入 `meta.startup_cleanup_version = FILE_FORMAT_VERSION`；
    /// 4. 再次 checkpoint/truncate，使完成标记本身不留在 WAL 里。
    ///
    /// **可重试且幂等**：完成标记只在三步都成功后才写；失败时标记缺失，下一次启动会重新执行。
    /// 已标记当前版本时直接返回 `false`，**不**重写数据库——重复打开不改变结构或数据。
    /// 返回 `true` 表示本次确实执行了重写。
    pub async fn run_startup_gate(&self) -> Result<bool, StorageError> {
        if !self.integrity_ok {
            return Err(StorageError::FailClosed(
                "startup cleanup refused on a store that failed its integrity check",
            ));
        }
        let completed =
            migrate::read_meta_row(&self.pools.write, migrate::META_STARTUP_CLEANUP_VERSION)
                .await?
                .and_then(|value| value.parse::<i64>().ok())
                .is_some_and(|version| version >= migrate::FILE_FORMAT_VERSION);
        if completed {
            return Ok(false);
        }
        let mut write = self.pools.write.acquire().await.db()?;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&mut *write)
            .await
            .db()?;
        sqlx::query("PRAGMA secure_delete = ON")
            .execute(&mut *write)
            .await
            .db()?;
        sqlx::query("VACUUM").execute(&mut *write).await.db()?;
        sqlx::query(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(migrate::META_STARTUP_CLEANUP_VERSION)
        .bind(migrate::FILE_FORMAT_VERSION.to_string())
        .execute(&mut *write)
        .await
        .db()?;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&mut *write)
            .await
            .db()?;
        drop(write);
        Ok(true)
    }

    /// 库级事实（§7.2 的 `meta`）。
    pub fn metadata(&self) -> &StoreMetadata {
        &self.meta
    }

    /// §7.1：关闭时执行一次 `wal_checkpoint(TRUNCATE)` 并关池。
    pub async fn close(self) {
        migrate::checkpoint_truncate(&self.pools.write).await;
        self.pools.close().await;
    }

    fn server_epoch(&self) -> Result<ServerEpoch, StorageError> {
        ServerEpoch::new(&self.meta.server_epoch).map_err(|_| StorageError::ColumnValue {
            column: "meta.server_epoch",
            expected: "canonical uuid",
        })
    }

    /// §7.1 失败关闭门：写路径的第一道检查（管理写集与 owned 写路径共用）。
    pub(crate) fn writable(&self) -> Result<(), PortError> {
        if self.integrity_ok {
            Ok(())
        } else {
            Err(PortError::Corrupt(
                "store is read-only after a failed integrity check",
            ))
        }
    }

    /// §7.5 的容量度量语句（`open` 时从 schema 现读两家族全部 TEXT 列拼出）。
    pub(crate) fn measure_sql(&self) -> &str {
        &self.measure_sql
    }

    /// §7.5 的保留窗口与容量上限（管理写集与 owned 写路径共用同一份策略）。
    pub(crate) fn window(&self) -> RetentionPolicy {
        RetentionPolicy {
            transcript_retention_days: self.config.transcript_retention_days,
            sync_event_retention_days: self.config.sync_event_retention_days,
            audit_retention_days: self.config.audit_retention_days,
            max_total_size_bytes: self.config.max_total_size_bytes,
            max_session_size_bytes: self.config.max_session_size_bytes,
            persist_deltas: self.config.persist_deltas,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 列读写辅助（显式列名 + 具名损坏错误）
// ---------------------------------------------------------------------------------------------

/// `sqlx::Error` 与端口错误之间隔着 [`StorageError`] 一层，而 `?` 只做一步转换；
/// `From<sqlx::Error> for PortError` 又是孤儿实现（两个类型都在外部 crate）。因此所有 sqlx 调用点都经
/// `.db()` 收敛成 `StorageError`，再由 `?` 交给端口边界（管理 store 模块共用）。
pub(crate) trait Db<T> {
    fn db(self) -> Result<T, StorageError>;
}

impl<T> Db<T> for Result<T, sqlx::Error> {
    fn db(self) -> Result<T, StorageError> {
        self.map_err(StorageError::from)
    }
}

pub(crate) fn text(row: &SqliteRow, column: &'static str) -> Result<String, StorageError> {
    row.try_get::<String, _>(column)
        .map_err(|_| StorageError::ColumnValue {
            column,
            expected: "non-null text",
        })
}

pub(crate) fn opt_text(
    row: &SqliteRow,
    column: &'static str,
) -> Result<Option<String>, StorageError> {
    row.try_get::<Option<String>, _>(column)
        .map_err(|_| StorageError::ColumnValue {
            column,
            expected: "text",
        })
}

/// BLOB 列（owned_peer_key.public_key / owned_device.public_key；§11.7 的跨族列类型约定）。
///
/// STRICT 表按列的亲和性校验入参，因此这里读不到 BLOB 就是库被外部改写：按列值损坏处理，不把
/// 字节以外的东西（hex 文本、错误长度的 BLOB）当成身份材料。
pub(crate) fn blob(row: &SqliteRow, column: &'static str) -> Result<Vec<u8>, StorageError> {
    row.try_get::<Vec<u8>, _>(column)
        .map_err(|_| StorageError::ColumnValue {
            column,
            expected: "blob",
        })
}

pub(crate) fn int(row: &SqliteRow, column: &'static str) -> Result<i64, StorageError> {
    row.try_get::<i64, _>(column)
        .map_err(|_| StorageError::ColumnValue {
            column,
            expected: "integer",
        })
}

fn opt_int(row: &SqliteRow, column: &'static str) -> Result<Option<i64>, StorageError> {
    row.try_get::<Option<i64>, _>(column)
        .map_err(|_| StorageError::ColumnValue {
            column,
            expected: "integer",
        })
}

fn opt_flag(row: &SqliteRow, column: &'static str) -> Result<Option<bool>, StorageError> {
    Ok(opt_int(row, column)?.map(|value| value != 0))
}

/// newtype / token enum 的统一解析：非法取值说明库被外部改写，按损坏处理。
pub(crate) fn decode<T>(value: &str, column: &'static str) -> Result<T, StorageError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value.parse::<T>().map_err(|_| StorageError::ColumnValue {
        column,
        expected: std::any::type_name::<T>(),
    })
}

pub(crate) fn decode_opt<T>(
    value: Option<String>,
    column: &'static str,
) -> Result<Option<T>, StorageError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    match value {
        Some(value) => decode(&value, column).map(Some),
        None => Ok(None),
    }
}

/// §3.2：`Sequence` 上界 `2^63-1`，库里出现更大或负数即损坏（不静默回绕）。
fn sequence(value: i64, column: &'static str) -> Result<Sequence, StorageError> {
    let value = u64::try_from(value).map_err(|_| StorageError::ColumnValue {
        column,
        expected: "non-negative sequence",
    })?;
    Sequence::new(value).map_err(|_| StorageError::ColumnValue {
        column,
        expected: "sequence within 2^63-1",
    })
}

fn parse_version(value: i64, column: &'static str) -> Result<Version, StorageError> {
    let value = u64::try_from(value).map_err(|_| StorageError::ColumnValue {
        column,
        expected: "non-negative version",
    })?;
    Ok(Version::new(value))
}

/// §7.3：`payload_digest` 与 ACP 原文摘要都是 32 字节 SHA-256 的规范 base64url（`Digest`）。
fn digest_text(bytes: &[u8]) -> Result<Digest, StorageError> {
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha256(bytes));
    Digest::new(&encoded).map_err(|_| StorageError::InvalidRequest("digest is not canonical"))
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes).into()
}

/// §7.3 的 `(actor_kind, actor_id)`：协议维度的幂等键，由 `Actor` 的既有分解给出。
pub(crate) fn actor_key(actor: &Actor) -> (ActorKind, String) {
    (actor.kind(), actor.id_text())
}

/// 从 `owned_command` 的 `(actor_kind, actor_id)` 还原 `Actor`。
///
/// `owned_command` 只存 §7.3 冻结的两列，因此还原只可能给出归因：设备记录不再有授权集合列，这里没有
/// 任何集合要补；Node 的复合键按 `"{node}/{access_node}"` 拆回。
///
/// `pairing_claimant` 在此**明确失败**：`owned_command.actor_kind` 的 CHECK 只有 `device`/`node`/`cli`
/// （认领方永不提交命令，design D12），真读到这一行说明库被外部改写——按列值损坏处理，不猜一个 actor。
fn actor_from_columns(kind: ActorKind, id: &str) -> Result<Actor, StorageError> {
    match kind {
        ActorKind::Device => Ok(Actor::Device {
            device: decode(id, "owned_command.actor_id")?,
        }),
        ActorKind::Node => {
            let (node, access_node) = id.split_once('/').ok_or(StorageError::ColumnValue {
                column: "owned_command.actor_id",
                expected: "node/access-node pair",
            })?;
            Ok(Actor::Node {
                node: decode(node, "owned_command.actor_id")?,
                access_node: decode(access_node, "owned_command.actor_id")?,
            })
        }
        ActorKind::Cli => Ok(Actor::LocalCli),
        ActorKind::PairingClaimant => Err(StorageError::ColumnValue {
            column: "owned_command.actor_kind",
            expected: "device, node or cli",
        }),
    }
}

/// `Cj1Error` → 固定的 `'static` 消息（`PortError::InvalidRequest` 只接受静态字符串）。
/// **不回显 payload 正文**（§8：错误消息不得含 prompt/正文）。
fn cj1_message(error: &acpr_wire::cj1::Cj1Error) -> &'static str {
    use acpr_wire::cj1::Cj1Error;
    match error {
        Cj1Error::InvalidUtf8(_) => "payload view is not valid UTF-8",
        Cj1Error::Bom => "payload view starts with a BOM",
        Cj1Error::Syntax { .. } => "payload view is not well-formed JSON",
        Cj1Error::ControlCharacter { .. } => "payload view contains an unescaped control character",
        Cj1Error::InvalidUnicodeEscape { .. } => "payload view contains an invalid \\u escape",
        Cj1Error::NotAContainer { .. } => "payload view must be a JSON object",
        Cj1Error::DuplicateKey { .. } => "payload view has a duplicate object key",
        Cj1Error::NonInteger { .. } => "payload view contains a non-integer number",
        Cj1Error::IntegerOutOfRange { .. } => "payload view number exceeds 2^53-1",
        Cj1Error::TooDeep { .. } => "payload view is nested too deeply",
    }
}

fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// §7.5：单条事件的过期时刻。
///
/// 窗口由**策略与类别共同**决定：`short_term`（delta）走 `sync_event_retention_days`，其余走
/// `transcript_retention_days`。这样既满足 §7.5 表格里按 `kind` 划定的清理集合（delta 7 天、
/// `state`/`final_message`/`structured`/`summary` 90 天），也不会让一个标成 durable 的 delta 逃过窗口。
fn expires_at(
    at: &Timestamp,
    policy: StoredPolicy,
    kind: EventKind,
    limits: &RetentionPolicy,
) -> Result<Timestamp, StorageError> {
    let days = match (policy, kind) {
        (StoredPolicy::ShortTerm, _) => limits.sync_event_retention_days,
        (_, EventKind::Delta) => limits.sync_event_retention_days,
        _ => limits.transcript_retention_days,
    };
    let shifted = window::shift_days(at.as_str(), i64::from(days)).ok_or(
        StorageError::InvalidRequest("timestamp is outside the supported window"),
    )?;
    Timestamp::new(&shifted).map_err(|_| StorageError::InvalidRequest("timestamp shape"))
}

/// `u64` → SQLite 可存的 `i64`。§3.2 的上界（`2^63-1`）保证正常路径不会溢出；真到溢出说明序号空间
/// 耗尽，按容量问题拒绝写入而不是回绕。
fn storable(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Unavailable(UnavailableKind::StorageFull))
}

fn sequence_of(value: u64) -> Result<Sequence, PortError> {
    Sequence::new(value).map_err(|_| PortError::Corrupt("sequence exceeds 2^63-1"))
}

/// 保留阈值：`at - days`（清理 `expires_at <= threshold` 的行）。
fn threshold(at: &Timestamp, days: u32) -> Result<Timestamp, StorageError> {
    let shifted = window::shift_days(at.as_str(), -i64::from(days)).ok_or(
        StorageError::InvalidRequest("timestamp is outside the supported window"),
    )?;
    Timestamp::new(&shifted).map_err(|_| StorageError::InvalidRequest("timestamp shape"))
}

// ---------------------------------------------------------------------------------------------
// 行 → 模型
// ---------------------------------------------------------------------------------------------

fn agent_from_row(row: &SqliteRow) -> Result<AgentRef, StorageError> {
    let id = text(row, "agent_id")?;
    let name = text(row, "agent_name")?;
    let id = AgentId::new(&id).map_err(|_| StorageError::ColumnValue {
        column: "owned_session.agent_id",
        expected: "agent id",
    })?;
    AgentRef::try_new(id, &name).map_err(|_| StorageError::ColumnValue {
        column: "owned_session.agent_name",
        expected: "agent name",
    })
}

fn mode_from_row(row: &SqliteRow) -> Result<Option<ModeRef>, StorageError> {
    match (
        opt_text(row, "current_mode_id")?,
        opt_text(row, "current_mode_name")?,
    ) {
        (None, None) => Ok(None),
        (Some(id), Some(name)) => {
            let id = ModeId::new(&id).map_err(|_| StorageError::ColumnValue {
                column: "owned_session.current_mode_id",
                expected: "mode id",
            })?;
            ModeRef::try_new(id, &name)
                .map(Some)
                .map_err(|_| StorageError::ColumnValue {
                    column: "owned_session.current_mode_name",
                    expected: "mode display name",
                })
        }
        _ => Err(StorageError::ColumnValue {
            column: "owned_session.current_mode_id",
            expected: "mode id and name together",
        }),
    }
}

/// 摘要里的目录归属（`SessionSummary.workspace`）。
///
/// 权威来源**只有** `owned_session.workspace_directory_id`：`NULL` 就是「未分组」（`None`），不按
/// `owned_session.workspace_cwd` 反查目录表补齐（`design.md` D1/D6、`workspace-resolution` 的
/// 「不按路径反查归属」）。
fn workspace_from_row(row: &SqliteRow) -> Result<Option<WorkspaceRef>, StorageError> {
    let Some(directory_id) = opt_text(row, "workspace_directory_id")? else {
        return Ok(None);
    };
    let directory_id = DirectoryId::new(&directory_id).map_err(|_| StorageError::ColumnValue {
        column: "owned_session.workspace_directory_id",
        expected: "directory id",
    })?;
    Ok(Some(WorkspaceRef::new(directory_id)))
}

fn session_summary_from_row(row: &SqliteRow) -> Result<SessionSummary, StorageError> {
    let summary = SessionSummary::try_new(
        decode(&text(row, "session_id")?, "owned_session.session_id")?,
        opt_text(row, "title")?,
        agent_from_row(row)?,
        decode(&text(row, "state")?, "owned_session.state")?,
        ResourceOrigin::Local,
        mode_from_row(row)?,
        parse_version(int(row, "version")?, "owned_session.version")?,
        decode(&text(row, "created_at")?, "owned_session.created_at")?,
        decode(&text(row, "updated_at")?, "owned_session.updated_at")?,
        workspace_from_row(row)?,
    )
    .map_err(|_| StorageError::ColumnValue {
        column: "owned_session",
        expected: "session summary invariants",
    })?;
    Ok(summary)
}

fn session_from_row(row: &SqliteRow) -> Result<Session, StorageError> {
    let id: SessionId = decode(&text(row, "session_id")?, "owned_session.session_id")?;
    let session = Session::try_new(
        id.clone(),
        acp_core::model::OwnedSessionRef::new(id),
        opt_text(row, "title")?,
        agent_from_row(row)?,
        decode(&text(row, "state")?, "owned_session.state")?,
        ResourceOrigin::Local,
        mode_from_row(row)?,
        parse_version(int(row, "version")?, "owned_session.version")?,
        decode(&text(row, "created_at")?, "owned_session.created_at")?,
        decode(&text(row, "updated_at")?, "owned_session.updated_at")?,
    )
    .map_err(|_| StorageError::ColumnValue {
        column: "owned_session",
        expected: "session invariants",
    })?;
    Ok(session)
}

fn turn_from_row(row: &SqliteRow) -> Result<Turn, StorageError> {
    let queue_index =
        u32::try_from(int(row, "queue_index")?).map_err(|_| StorageError::ColumnValue {
            column: "owned_turn.queue_index",
            expected: "u32",
        })?;
    Turn::try_new(
        decode(&text(row, "turn_id")?, "owned_turn.turn_id")?,
        decode(&text(row, "session_id")?, "owned_turn.session_id")?,
        decode(&text(row, "state")?, "owned_turn.state")?,
        queue_index,
        decode_opt(opt_text(row, "causation")?, "owned_turn.causation")?,
        decode_opt(opt_text(row, "started_at")?, "owned_turn.started_at")?,
        decode_opt(opt_text(row, "ended_at")?, "owned_turn.ended_at")?,
    )
    .map_err(|_| StorageError::ColumnValue {
        column: "owned_turn",
        expected: "turn invariants",
    })
}

/// 事件行的定位信息（**不含** payload：`CommittedEvent` 是 §3.4 冻结的只带定位的值对象，重放
/// （`ReplayBatch.events` 的 `CommittedDelivery::Owned`）也只用它）。
///
/// 正文（`payload_json`/`payload_digest`/`acp_*`）由本适配器按 §7.3 的列与 §9.9 的摘要规则写入；读取
/// 端有两条：按事件 id 的 `ReadView::event_payload`/`session_event_payload`，以及在**同一条行**里
/// 一并取出的 [`owned_event_from_row`]（会话历史读与 Node Link 切片，因此不需要逐条回查）。
fn event_from_row(row: &SqliteRow) -> Result<CommittedEvent, StorageError> {
    let event = CommittedEvent {
        event_type: decode(&text(row, "event_type")?, "owned_event.event_type")?,
        id: decode(&text(row, "event_id")?, "owned_event.event_id")?,
        session: decode_opt(opt_text(row, "session_id")?, "owned_event.session_id")?,
        session_sequence: match opt_int(row, "session_sequence")? {
            Some(value) => Some(sequence(value, "owned_event.session_sequence")?),
            None => None,
        },
        global_sequence: sequence(int(row, "global_sequence")?, "owned_event.global_sequence")?,
        origin_epoch: decode_opt(opt_text(row, "origin_epoch")?, "owned_event.origin_epoch")?,
        origin_sequence: match opt_int(row, "origin_sequence")? {
            Some(value) => Some(sequence(value, "owned_event.origin_sequence")?),
            None => None,
        },
        created_at: decode(&text(row, "created_at")?, "owned_event.created_at")?,
    };
    Ok(event)
}

/// `owned_event` 的一行 → 持久化正文（`ReadView::event_payload` 与 `session_event_payload` 共用）。
///
/// `view` 直接取库内 `payload_json` 的**原样文本**（只经 `ViewJson` 做形状校验，不重新序列化）；
/// `acp` 按 `acp_raw_unavailable_reason` 分两路还原（字节数必须与原文一致）。
fn payload_from_row(row: &SqliteRow) -> Result<EventPayload, PortError> {
    let view = ViewJson::new(&text(row, "payload_json")?)
        .map_err(|_| PortError::Corrupt("stored payload_json is not a JSON object"))?;
    let reason: Option<RawUnavailableReason> = decode_opt(
        opt_text(row, "acp_raw_unavailable_reason")?,
        "owned_event.acp_raw_unavailable_reason",
    )?;
    let acp = match reason {
        Some(reason) => {
            let byte_length = match opt_int(row, "acp_byte_length")? {
                Some(value) => u64::try_from(value)
                    .map_err(|_| PortError::Corrupt("stored acp_byte_length is negative"))?,
                None => 0,
            };
            let sha256 =
                decode_opt::<Digest>(opt_text(row, "acp_sha256")?, "owned_event.acp_sha256")?;
            Some(AcpRaw::unavailable(reason, byte_length, sha256))
        }
        None => match opt_text(row, "acp_raw_json")? {
            Some(raw_json) => {
                let media_type = text(row, "acp_media_type")?;
                let sha256 = decode::<Digest>(&text(row, "acp_sha256")?, "owned_event.acp_sha256")?;
                Some(
                    AcpRaw::available(&media_type, &raw_json, sha256)
                        .map_err(|_| PortError::Corrupt("stored ACP raw is inconsistent"))?,
                )
            }
            None => None,
        },
    };
    let payload = EventPayload::new(view, acp);
    payload
        .validate()
        .map_err(|_| PortError::Corrupt("stored event payload is inconsistent"))?;
    Ok(payload)
}

/// `owned_event` 的一行 → 定位 + 正文（会话历史读与 Node Link 切片共用）。
///
/// 两个读路径都用 `EVENT_COLUMNS` 一次取行：`payload_json` 与 ACP 五列**已经在这条行里**，这里只把
/// 内存里的两部分拼起来，因此不存在「先取信封、再按 id 逐条取正文」的 N+1（`design.md` D4）。
fn owned_event_from_row(row: &SqliteRow) -> Result<OwnedEventRecord, PortError> {
    Ok(OwnedEventRecord {
        event: event_from_row(row)?,
        payload: payload_from_row(row)?,
    })
}

/// `owned_event.origin_kind` 的解码（`SYNC_PROTOCOL.md` §10.1 的 `origin.kind`）。
///
/// **五个**取值（`agent`/`device`/`daemon`/`local_cli`/`node`，§7.3 的 CHECK 与 wire 词表同集）原样往返；
/// 词表外取值是**列值损坏**，不是「未知来源」——静默降级会让一条事件被当成别的来源交付，因此这里失败
/// 关闭。§7.3 的 CHECK 已经把该列钉在词表里，所以本分支只可能来自绕过 CHECK 的直接写入（读取侧的纵深
/// 防御）。
fn origin_kind_from_row(row: &SqliteRow) -> Result<EventOrigin, PortError> {
    decode(&text(row, "origin_kind")?, "owned_event.origin_kind")
        .map_err(|_| PortError::Corrupt("stored origin_kind is not a known event origin"))
}

/// `owned_command.actor_id` 上**节点**行的 actor id（`"{ownerNodeId}/{accessNodeId}"`，§7.3）→ 主
/// principal `ownerNodeId`。
///
/// Node Link 的幂等键是 `(ownerNodeId, accessNodeId, requestId)`（§6 第 6 条 / `Actor::id_text`），
/// §10.1 的 `origin.nodeId` 要的是产生该事件的另一个节点，即主 principal。复合键拆不开、或任一半不是
/// canonical UUID 时按列值损坏失败关闭（与设备行同款口径）。
fn node_id_from_composite_key(composite: &str) -> Result<NodeId, PortError> {
    let (node, _access_node) = composite.split_once('/').ok_or_else(|| {
        PortError::Corrupt("owned_command.actor_id is not a node/access-node pair")
    })?;
    NodeId::new(node).map_err(|_| PortError::Corrupt("owned_command.actor_id is not a node id"))
}

/// `owned_event` 的一行 → Sync 事件读面的条目（[`ReadView::sync_replay`]）。`origins` 见
/// [`command_origins`]。
fn sync_event_from_row(
    row: &SqliteRow,
    origins: &CommandOrigins,
) -> Result<SyncEventRecord, PortError> {
    let origin_kind = origin_kind_from_row(row)?;
    let event_id: EventId = decode(&text(row, "event_id")?, "owned_event.event_id")?;
    let causation: Option<RequestId> =
        decode_opt(opt_text(row, "causation")?, "owned_event.causation")?;
    // §10.1：`deviceId`/`nodeId` 只在对应 `origin.kind` 上出现；其余来源即便带同一个 causation 也
    // 不得回填，否则一条 CLI/Agent 事件会被交付成设备或节点产生的。得不出唯一 id 时失败关闭——§10.1
    // 的配对规则「`device` 语义上要求非空 deviceId」「`node` 必须带非空 nodeId」在本读面上只有这一
    // 条出路（见 [`identity_origin_for`]）。
    let origin_device_id = match origin_kind {
        EventOrigin::Device => {
            identity_origin_for(origins, "device", causation.as_ref(), &event_id, "device")?
                .map(|actor_id| {
                    DeviceId::new(&actor_id).map_err(|_| {
                        PortError::Corrupt("owned_command.actor_id is not a device id")
                    })
                })
                .transpose()?
        }
        EventOrigin::Agent | EventOrigin::Daemon | EventOrigin::LocalCli | EventOrigin::Node => {
            None
        }
    };
    let origin_node_id = match origin_kind {
        EventOrigin::Node => {
            identity_origin_for(origins, "node", causation.as_ref(), &event_id, "node")?
                .as_deref()
                .map(node_id_from_composite_key)
                .transpose()?
        }
        EventOrigin::Agent | EventOrigin::Daemon | EventOrigin::LocalCli | EventOrigin::Device => {
            None
        }
    };
    Ok(SyncEventRecord {
        event: event_from_row(row)?,
        payload: payload_from_row(row)?,
        origin_kind,
        origin_device_id,
        origin_node_id,
        causation,
    })
}

/// 本批 identity 类事件（`origin_kind ∈ {'device','node'}`）的 `causation`（= 诱发命令的
/// `requestId`）→ `owned_command.actor_id`（§10.1 的 `origin.deviceId`/`origin.nodeId`）。
///
/// 一次批量查询覆写 `actor_kind = 'device'` 与 `'node'` 两类 actor（一条 `IN (…)`，不是逐条事件查
/// 一次），与事件读共用调用方持有的同一个只读事务，因此「事件行 + 回查结果」来自同一快照。返回的键带
/// `actor_kind`：读条目时 device 事件只许看设备行、node 事件只许看节点行，两类身份因此不可能互换。
///
/// 同一 `requestId` 在同一 actor 类别下命中**多行**时值置 `None`：`owned_command` 的唯一键是
/// `(actor_kind, actor_id, request_id)`（§7.3），跨设备/跨节点复用同一 requestId 在 DDL 上合法，此时
/// 没有唯一答案。
#[derive(Default)]
struct CommandOrigins {
    requests: HashMap<(String, RequestId), Option<String>>,
    mkdir_terminals: HashMap<(String, EventId), String>,
}

async fn command_origins(
    ex: &mut SqliteConnection,
    rows: &[SqliteRow],
) -> Result<CommandOrigins, PortError> {
    let mut wanted: Vec<String> = Vec::new();
    for row in rows {
        if !matches!(
            origin_kind_from_row(row)?,
            EventOrigin::Device | EventOrigin::Node
        ) {
            continue;
        }
        if let Some(request) = opt_text(row, "causation")?
            && !wanted.iter().any(|known| known == &request)
        {
            wanted.push(request);
        }
    }
    let mut origins = CommandOrigins::default();
    if wanted.is_empty() {
        return Ok(origins);
    }
    let actor_kinds = placeholders(2);
    let requests = placeholders(wanted.len());
    let sql = format!(
        "SELECT actor_kind, actor_id, request_id, command, terminal_event_id FROM owned_command \
         WHERE actor_kind IN ({actor_kinds}) AND request_id IN ({requests})"
    );
    let mut statement = sqlx::query(&sql).bind("device").bind("node");
    for request in &wanted {
        statement = statement.bind(request.as_str());
    }
    let rows = statement.fetch_all(&mut *ex).await.db()?;
    for row in &rows {
        let kind = text(row, "actor_kind")?;
        let request: RequestId = decode(&text(row, "request_id")?, "owned_command.request_id")?;
        let actor_id = text(row, "actor_id")?;
        // mkdir terminals have a durable, exact link even when request ids collide across devices.
        if text(row, "command")? == "workspace.mkdir" {
            if let Some(event_id) = opt_text(row, "terminal_event_id")? {
                let event_id: EventId = decode(&event_id, "owned_command.terminal_event_id")?;
                origins
                    .mkdir_terminals
                    .insert((kind.clone(), event_id), actor_id.clone());
            }
        }
        match origins.requests.entry((kind, request)) {
            Entry::Vacant(slot) => {
                slot.insert(Some(actor_id));
            }
            // 与已登记的那个不同 = 同一 requestId 在同一 actor 类别下命中多行：唯一键允许这种形状
            // （§7.3），此时没有唯一答案。
            Entry::Occupied(mut slot) if slot.get().as_ref() != Some(&actor_id) => {
                slot.insert(None);
            }
            Entry::Occupied(_) => {}
        }
    }
    Ok(origins)
}

/// `origin.deviceId`/`origin.nodeId` 的回查结果：得不出**唯一**取值时失败关闭
/// （[`PortError::Corrupt`]）。
///
/// 三类「无法归因」在**升级前写入的行**上都会真实出现（它们正是 4.8 要求显式处置的历史行）：
/// ① `causation` 为空——1.7 之前 agent-host 产出的每条 `EndpointEvent` 都不带 `causation`
///   （`mapper.rs` 的唯一生产构造点固定 `None`）、broker 又原样落盘，这类行连回查的键都没有；
/// ② 该 requestId 在本机 `owned_command` 里没有对应行——命令不在本机历史窗口内（全仓没有任何
///   `owned_command` 清理路径，但事件与命令**本来就可能不在同一节点上产生**），或该事件由另一条
///   命令路径产生；
/// ③ 命令行存在但 `actor_kind` 不是本事件要求的类别——诱发命令来自本机 CLI（`cli`）或另一个节点
///   （`node`）而事件仍被标成另一类。
///
/// 这些情形一律不得从旁路（会话归属、actor 瞬时状态……）推断一个 id：编造 id 比失败关闭更糟——它会把
/// 归属判错给另一台设备/另一个节点，而调用方无法察觉。
fn identity_origin_for(
    origins: &CommandOrigins,
    actor_kind: &str,
    causation: Option<&RequestId>,
    event_id: &EventId,
    kind: &str,
) -> Result<Option<String>, PortError> {
    if let Some(actor_id) = origins
        .mkdir_terminals
        .get(&(actor_kind.to_owned(), event_id.clone()))
    {
        return Ok(Some(actor_id.clone()));
    }
    let reason = match kind {
        "device" => "device event cannot be attributed to a unique device command",
        _ => "node event cannot be attributed to a unique node command",
    };
    match causation.map(|request| {
        origins
            .requests
            .get(&(actor_kind.to_owned(), request.clone()))
    }) {
        // `causation` 为空、或该 key 没有行（命令行不在本机/不是本类别 actor）⇒ 无法归因。
        None | Some(None) => Err(PortError::Corrupt(reason)),
        // 有条目但值为 `None` = 同一 requestId 在同一 actor 类别下命中多行（`command_origins`）⇒
        // 同样没有唯一答案。
        Some(Some(None)) => Err(PortError::Corrupt(reason)),
        Some(Some(Some(actor_id))) => Ok(Some(actor_id.clone())),
    }
}

fn command_record_from_row(row: &SqliteRow) -> Result<CommandRecord, StorageError> {
    let actor_kind: ActorKind = decode(&text(row, "actor_kind")?, "owned_command.actor_kind")?;
    let actor_id = text(row, "actor_id")?;
    let result =
        match opt_text(row, "result_json")? {
            Some(text) => Some(CommandResult::from_json_text(&text).map_err(|_| {
                StorageError::ColumnValue {
                    column: "owned_command.result_json",
                    expected: "JSON object",
                }
            })?),
            None => None,
        };
    let error = match opt_text(row, "error_code")? {
        Some(code) => {
            let message = opt_text(row, "error_message")?.unwrap_or_default();
            let retryable = opt_flag(row, "retryable")?.unwrap_or(false);
            let details = match opt_text(row, "error_details_json")? {
                Some(text) => ViewJson::new(&text).map_err(|_| StorageError::ColumnValue {
                    column: "owned_command.error_details_json",
                    expected: "JSON object",
                })?,
                None => ViewJson::empty_object(),
            };
            Some(
                PublicError::try_new(&code, &message, retryable, details).map_err(|_| {
                    StorageError::ColumnValue {
                        column: "owned_command.error_code",
                        expected: "error code",
                    }
                })?,
            )
        }
        None => None,
    };
    let record = CommandRecord::try_new(
        decode_opt(opt_text(row, "session_id")?, "owned_command.session_id")?,
        decode(&text(row, "request_id")?, "owned_command.request_id")?,
        &text(row, "command")?,
        decode(&text(row, "kind")?, "owned_command.kind")?,
        actor_from_columns(actor_kind, &actor_id)?,
        decode_opt(opt_text(row, "accepted_at")?, "owned_command.accepted_at")?,
        decode(&text(row, "status")?, "owned_command.status")?,
        decode_opt(opt_text(row, "terminal_at")?, "owned_command.terminal_at")?,
        decode_opt::<EventId>(
            opt_text(row, "terminal_event_id")?,
            "owned_command.terminal_event_id",
        )?,
        result,
        error,
        match opt_int(row, "expected_version")? {
            Some(value) => Some(parse_version(value, "owned_command.expected_version")?),
            None => None,
        },
        decode(
            &text(row, "request_fingerprint")?,
            "owned_command.request_fingerprint",
        )?,
    )
    .map_err(|_| StorageError::ColumnValue {
        column: "owned_command",
        expected: "command record invariants",
    })?;
    Ok(record)
}

// ---------------------------------------------------------------------------------------------
// SQL 片段
// ---------------------------------------------------------------------------------------------

const TURN_COLUMNS: &str =
    "turn_id, session_id, state, queue_index, causation, started_at, ended_at";
const INTERACTION_COLUMNS: &str = "interaction_id, kind, session_id, created_at";
const INSERT_EVENT: &str = "INSERT INTO owned_event (global_sequence, session_id, session_sequence, \
     origin_epoch, origin_sequence, event_id, turn_id, event_type, kind, policy, origin_kind, \
     causation, payload_json, payload_digest, acp_media_type, acp_raw_json, acp_byte_length, \
     acp_sha256, acp_raw_unavailable_reason, created_at, expires_at, compacted_into) \
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)";

const INSERT_COMMAND: &str = "INSERT INTO owned_command (actor_kind, actor_id, request_id, \
     session_id, command, kind, expected_version, request_fingerprint, accepted_at, status, \
     terminal_at, terminal_event_id, result_json, error_code, error_message, error_details_json, \
     retryable) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// §7.5 的容量度量：`owned_*` **所有 TEXT 列**的 `length()` 之和，加上
/// `owned_attachment.byte_length` 之和（不含 WAL/freelist）。
///
/// 列清单在运行时从 `pragma_table_info` 取（只取 `type = 'TEXT'`），因此新增列自动计入。
///
/// 与 `StoreHealth::total_bytes` 同口径：容量测试的预算从该字段派生，不再各自实现一份度量。
async fn build_measure_sql(ex: &mut SqliteConnection) -> Result<String, StorageError> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(&mut *ex)
    .await
    .db()?;
    let mut terms = Vec::new();
    for table in tables {
        let columns: Vec<(String, String)> =
            sqlx::query_as("SELECT name, type FROM pragma_table_info(?1)")
                .bind(&table)
                .fetch_all(&mut *ex)
                .await
                .db()?;
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
    // 附件字节不在 TEXT 列里（`owned_attachment.relative_path` 只是路径），单列累加。
    terms.push("COALESCE((SELECT SUM(byte_length) FROM owned_attachment), 0)".to_owned());
    Ok(format!("SELECT {}", terms.join(" + ")))
}

const MEASURE_SESSION: &str = "SELECT COALESCE(SUM(length(payload_json)), 0) + \
     COALESCE(SUM(length(acp_raw_json)), 0) FROM owned_event WHERE session_id = ?1";

/// 事件字节数的度量（与删除用同一谓词，保证 `freed_bytes` 与实际删除一致）。
fn measure_events_sql(predicate: &str) -> String {
    format!(
        "SELECT COUNT(*), COALESCE(SUM(length(payload_json)), 0) + \
         COALESCE(SUM(length(acp_raw_json)), 0) FROM owned_event WHERE {predicate}"
    )
}

fn delete_events_sql(predicate: &str) -> String {
    format!("DELETE FROM owned_event WHERE {predicate}")
}

fn placeholders(count: usize) -> String {
    let mut out = String::new();
    for index in 0..count {
        if index > 0 {
            out.push_str(", ");
        }
        out.push('?');
    }
    out
}

/// 执行 `open` 时构建好的度量语句（§7.5）。
async fn measure_total(ex: &mut SqliteConnection, measure_sql: &str) -> Result<u64, StorageError> {
    let total: i64 = sqlx::query_scalar(measure_sql).fetch_one(ex).await.db()?;
    u64::try_from(total).map_err(|_| StorageError::ColumnValue {
        column: "storage size",
        expected: "non-negative byte count",
    })
}

async fn measure_session(
    ex: &mut SqliteConnection,
    session: &SessionId,
) -> Result<u64, StorageError> {
    let bytes: i64 = sqlx::query_scalar(MEASURE_SESSION)
        .bind(session.as_str())
        .fetch_one(&mut *ex)
        .await
        .db()?;
    u64::try_from(bytes).map_err(|_| StorageError::ColumnValue {
        column: "session size",
        expected: "non-negative byte count",
    })
}

async fn next_global_sequence(ex: &mut SqliteConnection) -> Result<u64, StorageError> {
    let value: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(global_sequence), 0) + 1 FROM owned_event")
            .fetch_one(ex)
            .await
            .db()?;
    u64::try_from(value).map_err(|_| StorageError::OutOfRange("global sequence exhausted"))
}

async fn next_sequence_for(
    ex: &mut SqliteConnection,
    column: &str,
    session: &SessionId,
) -> Result<u64, StorageError> {
    let value: i64 = sqlx::query_scalar(&format!(
        "SELECT COALESCE(MAX({column}), 0) + 1 FROM owned_event WHERE session_id = ?1"
    ))
    .bind(session.as_str())
    .fetch_one(ex)
    .await
    .db()?;
    u64::try_from(value).map_err(|_| StorageError::OutOfRange("session sequence exhausted"))
}

async fn session_version_and_epoch(
    ex: &mut SqliteConnection,
    session: &SessionId,
) -> Result<Option<(Version, OriginEpoch)>, StorageError> {
    let row = sqlx::query("SELECT version, origin_epoch FROM owned_session WHERE session_id = ?1")
        .bind(session.as_str())
        .fetch_optional(ex)
        .await
        .db()?;
    match row {
        Some(row) => Ok(Some((
            parse_version(int(&row, "version")?, "owned_session.version")?,
            decode(&text(&row, "origin_epoch")?, "owned_session.origin_epoch")?,
        ))),
        None => Ok(None),
    }
}

async fn load_command_row(
    ex: &mut SqliteConnection,
    actor_kind: &str,
    actor_id: &str,
    request: &str,
) -> Result<Option<SqliteRow>, StorageError> {
    let row = sqlx::query(&format!(
        "SELECT {COMMAND_COLUMNS} FROM owned_command \
         WHERE actor_kind = ?1 AND actor_id = ?2 AND request_id = ?3"
    ))
    .bind(actor_kind)
    .bind(actor_id)
    .bind(request)
    .fetch_optional(ex)
    .await
    .db()?;
    Ok(row)
}

async fn read_metadata(pool: &SqlitePool) -> Result<StoreMetadata, StorageError> {
    let read = |key: &'static str| async move {
        migrate::read_meta_row(pool, key)
            .await?
            .ok_or(StorageError::Corrupt("meta row is missing"))
    };
    let server_epoch = read(migrate::META_SERVER_EPOCH).await?;
    let created_at = read(migrate::META_CREATED_AT).await?;
    let last_prune_at = read(migrate::META_LAST_PRUNE_AT).await?;
    Ok(StoreMetadata {
        server_epoch,
        owned_schema_version: migrate::OWNED_SCHEMA_VERSION,
        created_at,
        last_prune_at,
    })
}

// ---------------------------------------------------------------------------------------------
// SessionStore
// ---------------------------------------------------------------------------------------------

/// §6 第 6 条：幂等键相同也要逐项比对，任一项不同即 `IdempotencyConflict`。
fn verify_idempotent(
    row: &SqliteRow,
    idem: &acp_core::ports::IdempotencyRecord,
) -> Result<(), PortError> {
    let stored_session = opt_text(row, "session_id").map_err(PortError::from)?;
    let stored_expected = opt_int(row, "expected_version").map_err(PortError::from)?;
    // `idem.session = None` 表示「该命令没有装配期可知的目标会话」（目前只有 `session.create`：它的
    // `session_id` 由存储层在创建事务内分配，见 §6 第 20 条），此时不参与比对——否则同键重试会被误判为冲突。
    let session_matches = match idem.session.as_ref() {
        Some(session) => stored_session.as_deref() == Some(session.as_str()),
        None => true,
    };
    let same = text(row, "command").map_err(PortError::from)? == idem.command
        && text(row, "kind").map_err(PortError::from)? == idem.kind.as_str()
        && session_matches
        && stored_expected.map(|value| value.to_string())
            == idem.expected_version.map(|value| value.to_string())
        && text(row, "request_fingerprint").map_err(PortError::from)?
            == idem.request_fingerprint.as_str();
    if same {
        Ok(())
    } else {
        Err(PortError::Conflict(ConflictKind::IdempotencyConflict))
    }
}

impl SqliteStore {
    async fn commit_owned(
        &self,
        commit: OwnedCommit,
        terminal_actor: Option<&Actor>,
        binding: Option<SubmissionBinding>,
    ) -> Result<CommitOutcome, PortError> {
        // §6 第 13 条：创建 pending 交互行（`interactions`）与解析既有行（`state.interaction`）
        // 是互斥的两条路径，同一提交里同时出现即拒绝。
        let resolving = matches!(
            &commit.state,
            Some(StateChange::Update(update)) if update.interaction.is_some()
        );
        if !commit.interactions.is_empty() && resolving {
            return Err(PortError::InvalidRequest(
                "a commit cannot both create and resolve an interaction",
            ));
        }

        // §6 第 15 条：压缩收据的三条前置规则（任一不满足都是整事务 `InvalidRequest`）。
        if !commit.compacted.is_empty() {
            if self.config.persist_deltas {
                return Err(PortError::InvalidRequest(
                    "delta compaction is disabled while storage.persist_deltas is true",
                ));
            }
            let summary_events = commit
                .events
                .iter()
                .filter(|event| event.kind == EventKind::Summary)
                .count();
            if summary_events != 1 {
                return Err(PortError::InvalidRequest(
                    "a compaction commit must carry exactly one summary event",
                ));
            }
            if commit.session.is_none() {
                return Err(PortError::InvalidRequest(
                    "a compaction commit requires a session id",
                ));
            }
        }

        if terminal_actor.is_some()
            && (commit.session.is_some()
                || commit.idempotency.is_some()
                || commit.command_terminal.is_none()
                || commit.state.is_some()
                || !commit.turns.is_empty()
                || !commit.interactions.is_empty()
                || !commit.compacted.is_empty()
                || commit.origin_epoch.is_some())
        {
            return Err(PortError::InvalidRequest(
                "invalid workspace.mkdir terminal commit",
            ));
        }

        let window = self.window();
        let mut tx = self.pools.write.begin_with("BEGIN IMMEDIATE").await.db()?;

        // ---- 幂等：键已存在即重放（§6 第 6 条），不追加事件、不改状态
        if let Some(idem) = commit.idempotency.as_ref() {
            let (kind, actor_id) = actor_key(&idem.actor);
            if let Some(row) =
                load_command_row(&mut tx, kind.as_str(), &actor_id, idem.request.as_str()).await?
            {
                verify_idempotent(&row, idem)?;
                let record = command_record_from_row(&row)?;
                let (version, origin_epoch) = match idem.session.as_ref() {
                    Some(session) => match session_version_and_epoch(&mut tx, session).await? {
                        Some((version, epoch)) => (version, Some(epoch)),
                        None => (Version::new(0), None),
                    },
                    None => (Version::new(0), None),
                };
                // 命中路径不写任何行，显式结束事务。§6 第 20 条：`idem.session` 为 `None` 时以行里记的
                // 目标会话作答（`session.create` 的重试因此能拿回首次创建的那个 sessionId）。
                let session_id = idem.session.clone().or_else(|| record.session().cloned());
                drop(tx);
                return Ok(CommitOutcome {
                    session_id,
                    origin_epoch,
                    version,
                    appended: Vec::new(),
                    replayed: Some(IdempotentReplay { record }),
                });
            }
        }

        // ---- 附件清单绑定（§6 第 22 条）：与受理行、事件、turn **同一事务**。
        //
        // 幂等命中已在上面提前返回，因此重试不会因「清单已过期」被误拒：首次受理的结果就是首次结果。
        // 本段任一失败都让整事务回滚，调用方不会派发 Agent。
        if let Some(binding) = &binding {
            let session = commit.session.clone().ok_or(PortError::InvalidRequest(
                "attachment binding requires a session",
            ))?;
            if commit.idempotency.is_none() || commit.command_terminal.is_some() {
                return Err(PortError::InvalidRequest(
                    "attachment binding is only valid on an acceptance commit",
                ));
            }
            bind_submission(&mut tx, &session, binding, &self.config, &commit.at).await?;
        }

        // ---- 命令**终态**提交：`command_terminal` 自身不带 requestId，靠同一提交里
        // `event_type ∈ {command.completed, command.failed, command.uncertain}` 且
        // `causation = requestId` 的事件定位既有 `owned_command` 行（§10.1/§11.2）。
        // 定位必须在写事件之前完成，否则重发的终态块会把事件重复追加一遍。
        //
        // §6 第 20 条：定位按提交形状分三条，**都只认 `accepted` 行**（终态是幂等的：重复投递按重放
        // 回既有结果，不改行、不追加事件）：
        // ① `SettleCreate`：会话行与终态同批落盘，因此目标行是受理提交写的**尚无会话**那一行
        //    （`session_id IS NULL`），本事务稍后回填它的 `session_id`；
        // ② 其它带会话的终态提交（`session.resume`、prompt 等）：`(session_id, request_id)`；
        // ③ `session` 为 `None`（`session.create` 的失败终态、启动恢复）：`request_id + session_id IS NULL`。
        // ②③ 命中多行即整事务 `InvalidRequest`（同一 `requestId` 在多条 actor 下各有一行，不猜）。
        let settlement = matches!(&commit.state, Some(StateChange::SettleCreate(_)));
        let terminal_request = match (&commit.idempotency, &commit.command_terminal) {
            (None, Some(_)) => {
                let request =
                    terminal_request_of(&commit.events).ok_or(PortError::InvalidRequest(
                        "a command terminal requires its terminal event with causation = requestId",
                    ))?;
                let session = commit.session.clone();
                let located = if settlement {
                    if session.is_none() {
                        return Err(PortError::InvalidRequest(
                            "a settle-create commit requires a session id",
                        ));
                    }
                    locate_unsessioned_command(&mut tx, &request).await?
                } else {
                    match session.as_ref() {
                        Some(session) => locate_session_command(&mut tx, session, &request).await?,
                        None => match terminal_actor {
                            Some(actor) => {
                                let (kind, id) = actor_key(actor);
                                let row =
                                    load_command_row(&mut tx, kind.as_str(), &id, request.as_str())
                                        .await?;
                                if let Some(row) = row.as_ref() {
                                    if text(row, "command")? != "workspace.mkdir"
                                        || opt_text(row, "session_id")?.is_some()
                                    {
                                        return Err(PortError::InvalidRequest(
                                            "workspace.mkdir terminal targets another command",
                                        ));
                                    }
                                }
                                row
                            }
                            None => locate_unsessioned_command(&mut tx, &request).await?,
                        },
                    }
                };
                let Some(row) = located else {
                    return Err(PortError::NotFound(EntityRef::Command { session, request }));
                };
                let status: CommandStatus = decode(&text(&row, "status")?, "owned_command.status")?;
                if status != CommandStatus::Accepted {
                    // 同一终态块被重复投递：不追加事件、不改行，按重放回既有结果。
                    let record = command_record_from_row(&row)?;
                    let (version, origin_epoch) = match record.session().cloned() {
                        Some(session) => {
                            match session_version_and_epoch(&mut tx, &session).await? {
                                Some((version, epoch)) => (version, Some(epoch)),
                                None => (Version::new(0), None),
                            }
                        }
                        None => (Version::new(0), None),
                    };
                    drop(tx);
                    return Ok(CommitOutcome {
                        session_id: record.session().cloned(),
                        origin_epoch,
                        version,
                        appended: Vec::new(),
                        replayed: Some(IdempotentReplay { record }),
                    });
                }
                Some(request)
            }
            _ => None,
        };

        // ---- 会话状态与版本
        let mut session_id = commit.session.clone();
        let mut origin_epoch = commit.origin_epoch.clone();
        let version: Version;
        match &commit.state {
            Some(StateChange::Create(new)) => {
                if commit.session.is_some() || commit.expected_version.is_some() {
                    return Err(PortError::InvalidRequest(
                        "session creation must not carry a session id or an expected version",
                    ));
                }
                let epoch = committed_epoch(&commit.origin_epoch)?;
                let id_text = new_uuid();
                let id = SessionId::new(&id_text)
                    .map_err(|_| PortError::InvalidRequest("generated session id is malformed"))?;
                insert_session_row(
                    &mut tx, &id_text, &new.title, &new.agent, &epoch, &commit.at,
                )
                .await?;
                session_id = Some(id);
                origin_epoch = Some(epoch);
                version = Version::new(1);
            }
            Some(StateChange::SettleCreate(settled)) => {
                // §6 第 20 条：core 已分配会话标识，存储层**采用**它（`CommitOutcome.session_id` 原样
                // 回传），并把 ACP 会话标识 / 创建时 cwd / 目录别名与终态在同一次提交里落盘。
                if commit.session.as_ref() != Some(&settled.session) {
                    return Err(PortError::InvalidRequest(
                        "a settle-create commit must carry the settled session id",
                    ));
                }
                if commit.expected_version.is_some()
                    || commit.idempotency.is_some()
                    || commit.command_terminal.is_none()
                {
                    return Err(PortError::InvalidRequest(
                        "a settle-create commit carries exactly the row, the idempotency backfill \
                         and the command terminal",
                    ));
                }
                let epoch = committed_epoch(&commit.origin_epoch)?;
                insert_created_session_row(&mut tx, settled, &epoch, &commit.at).await?;
                // 回填受理行的 `session_id`：终态定位（本函数的 ①）与启动恢复都按会话标识找它，
                // 回填前那两处定位不到。行必须在 `apply_terminal` **之前**回填（后者的条件更新按
                // 会话标识过滤）。
                if let Some(request) = &terminal_request {
                    backfill_command_session(&mut tx, request, &settled.session).await?;
                }
                session_id = Some(settled.session.clone());
                origin_epoch = Some(epoch);
                version = Version::new(1);
            }
            Some(StateChange::Update(update)) => {
                let session = commit.session.clone().ok_or(PortError::InvalidRequest(
                    "session update requires a session id",
                ))?;
                let (current, stored_epoch) =
                    session_version_and_epoch(&mut tx, &session)
                        .await?
                        .ok_or_else(|| PortError::NotFound(EntityRef::Session(session.clone())))?;
                if commit
                    .expected_version
                    .is_some_and(|expected| expected != current)
                {
                    return Err(PortError::Conflict(ConflictKind::VersionMismatch));
                }
                if commit
                    .origin_epoch
                    .as_ref()
                    .is_some_and(|epoch| epoch != &stored_epoch)
                {
                    return Err(PortError::Conflict(ConflictKind::VersionMismatch));
                }
                // 调用方未提供 epoch 时用库内值（与无状态变更的分支、以及 §5.2 的
                // 「存储层只校验已有 epoch 时必须一致」一致）：否则同批的会话级事件会因缺 epoch 被拒。
                origin_epoch.get_or_insert(stored_epoch);
                // `SessionUpdate.state` 是 `Option`：`None` = 不改状态（例如只解析一个交互）。
                // 状态只有三态（`idle|running|pending`），取值由 core 的 `SessionActivity::reduce` 统一
                // 计算；存储层不再有「关闭」这一层语义，也不写任何关闭时间列。
                let state = update.state.map(SessionState::as_str);
                let (mode_id, mode_name) = mode_columns(&update.mode);
                // `CASE WHEN ?x IS NULL` 让「未提供的字段保持原值」。恢复所需的 `agent_session_id`/
                // `workspace_cwd` 与目录归属的 `workspace_directory_id` 走同一条规则（§7.2 的 v5/v6 三列）：
                // `None` = 不改该列，因此恢复流程（它永远传 `None`）不可能覆写已持久化的取值（spec R22）。
                // 恢复两列不在任何读投影里（§3.6 的窄读取）；`workspace_directory_id` 只由摘要投影消费。
                // 参数编号按各语句自己连续编号：SQLite 的 `?NNN` 是按位置绑定，中间留空会让后面的
                // 绑定落到未使用的下标上（`?7 IS NULL` 恒真 → 静默不写）。
                let statement = match &update.mode {
                    ModeChange::Unchanged => {
                        "UPDATE owned_session SET \
                         state = CASE WHEN ?1 IS NULL THEN state ELSE ?1 END, \
                         agent_session_id = CASE WHEN ?2 IS NULL THEN agent_session_id ELSE ?2 END, \
                         workspace_cwd = CASE WHEN ?3 IS NULL THEN workspace_cwd ELSE ?3 END, \
                         workspace_directory_id = CASE WHEN ?4 IS NULL THEN workspace_directory_id ELSE ?4 END, \
                         title = CASE WHEN ?6 THEN NULL WHEN ?5 IS NULL THEN title ELSE ?5 END, \
                         version = version + 1, updated_at = ?7 \
                         WHERE session_id = ?8 RETURNING version"
                    }
                    ModeChange::Set(_) => {
                        "UPDATE owned_session SET \
                         state = CASE WHEN ?1 IS NULL THEN state ELSE ?1 END, \
                         agent_session_id = CASE WHEN ?4 IS NULL THEN agent_session_id ELSE ?4 END, \
                         workspace_cwd = CASE WHEN ?5 IS NULL THEN workspace_cwd ELSE ?5 END, \
                         workspace_directory_id = CASE WHEN ?6 IS NULL THEN workspace_directory_id ELSE ?6 END, \
                         title = CASE WHEN ?8 THEN NULL WHEN ?7 IS NULL THEN title ELSE ?7 END, \
                         version = version + 1, updated_at = ?9, \
                         current_mode_id = ?2, current_mode_name = ?3 \
                         WHERE session_id = ?10 RETURNING version"
                    }
                };
                let mut query = sqlx::query_scalar::<_, i64>(statement).bind(state);
                if let ModeChange::Set(_) = &update.mode {
                    query = query.bind(mode_id).bind(mode_name);
                }
                let query = query
                    .bind(update.agent_session_id.as_ref().map(AgentSessionId::as_str))
                    .bind(update.workspace_cwd.as_deref())
                    .bind(update.workspace_directory_id.as_deref());
                // R9（`design.md` D7）：标题是**两层可选**——`None` = 不改该列，`Some(None)` = 显式置空，
                // `Some(Some(text))` = 写入该标题。SQLite 的 `?N IS NULL` 区分不了「不改」与「置空」，因此
                // 借一条按真值判定的**哨兵**参数（`ModeChange::Unchanged` 下是 `?6`，`Set` 下是 `?8`）把
                // 「不改」编码成「哨兵 = 0 且值为 NULL」、把「置空」编码成「哨兵 = 1」：一个 `CASE` 同时
                // 表达三态，与其余列共用**同一条**语句，因此 `version = version + 1`、`updated_at` 与同一
                // 提交里的其它列一律照写（§5.2/§6 第 21 条）。
                let clear_title = i64::from(matches!(update.title, Some(None)));
                let query = query
                    .bind(update.title.clone().flatten())
                    .bind(clear_title)
                    .bind(commit.at.as_str())
                    .bind(session.as_str());
                let new_version: Option<i64> = query.fetch_optional(&mut *tx).await.db()?;
                version = match new_version {
                    Some(value) => parse_version(value, "owned_session.version")?,
                    None => return Err(PortError::NotFound(EntityRef::Session(session.clone()))),
                };
                // §5.2/§6.7：交互解析用条件更新实现 first-writer-wins，与状态修改同一事务。
                if let Some(resolved) = &update.interaction {
                    resolve_interaction(&mut tx, &session, resolved, &commit.at).await?;
                }
                // 应答仲裁不等于处理完成：`confirmation` 在同一事务里写 `confirmed_at`，该交互此后
                // 才退出有效集合（`session-activity` D2）。
                if let Some(confirmation) = &update.confirmation {
                    confirm_interaction(&mut tx, &session, confirmation, &commit.at).await?;
                }
            }
            None => match &commit.session {
                Some(session) => {
                    let (current, stored_epoch) = session_version_and_epoch(&mut tx, session)
                        .await?
                        .ok_or_else(|| PortError::NotFound(EntityRef::Session(session.clone())))?;
                    if commit
                        .origin_epoch
                        .as_ref()
                        .is_some_and(|epoch| epoch != &stored_epoch)
                    {
                        return Err(PortError::Conflict(ConflictKind::VersionMismatch));
                    }
                    origin_epoch.get_or_insert(stored_epoch);
                    version = current;
                }
                None => version = Version::new(0),
            },
        }

        // ---- turn 变更（queue_index 由存储层按会话分配：`NewTurn` 不携带它）
        for change in &commit.turns {
            let session = commit.session.clone().ok_or(PortError::InvalidRequest(
                "a turn change requires a session id",
            ))?;
            match change {
                TurnChange::Create(new_turn) => {
                    if new_turn.state.is_terminal() {
                        return Err(PortError::InvalidRequest(
                            "a new turn cannot start in a terminal state",
                        ));
                    }
                    insert_turn(&mut tx, &session, new_turn).await?;
                }
                TurnChange::Update(update) => {
                    if update.state.is_terminal() != update.ended_at.is_some() {
                        return Err(PortError::InvalidRequest(
                            "a terminal turn state requires an end timestamp",
                        ));
                    }
                    update_turn(&mut tx, &session, update).await?;
                }
            }
        }

        // turn 终结时，它名下的有效请求在同一事务里失效（`session-activity` D2）：任务结束/失败/取消
        // 都不该留下永远等待的卡片。只失效**本 turn** 的行——按 `turn_id` 关联，绝不误伤其它 turn；
        // 失效范围是仍未解决的行（`pending` 或已仲裁未确认），已确认/已失效的行保持原状。
        for change in &commit.turns {
            if let TurnChange::Update(update) = change
                && update.state.is_terminal()
            {
                expire_turn_interactions(&mut tx, &update.turn, &commit.at).await?;
            }
        }

        // ---- 事件（会话级序号与全局序号在同一事务内分配）
        let mut appended = Vec::with_capacity(commit.events.len());
        let mut terminal_event_id: Option<EventId> = None;
        let mut summary_sequence: Option<Sequence> = None;
        if !commit.events.is_empty() {
            let mut global = next_global_sequence(&mut tx).await?;
            // `session_sequence` 与 `origin_sequence` 是**各自独立**的会话级序号（§3.4）：都从
            // `MAX+1` 起算并各自 +1，不得互相替代。
            let mut session_sequence = 0_u64;
            let mut origin_sequence = 0_u64;
            if let Some(session) = &session_id {
                session_sequence = next_sequence_for(&mut tx, "session_sequence", session).await?;
                origin_sequence = next_sequence_for(&mut tx, "origin_sequence", session).await?;
            }
            for pending in &commit.events {
                pending
                    .validate()
                    .map_err(|_| PortError::InvalidRequest("event payload is inconsistent"))?;
                let event_id_text = new_uuid();
                // §7.3/§9.9：`payload_digest` 是**对 `payload_json` 施 ACPR-CJ1 后的** SHA-256，
                // 不是对库内原始字节取摘要——`payload_json` 按 §9.16 原样保留调用方字节（键序、
                // 未知字段都不重排），所以只有规范化之后两边才算得一致（跨节点复算依赖它）。
                let canonical = acpr_wire::cj1::canonicalize(pending.payload.view.as_str())
                    .map_err(|error| PortError::InvalidRequest(cj1_message(&error)))?;
                let payload_digest = digest_text(canonical.as_bytes())?;
                let expires = expires_at(&commit.at, pending.policy, pending.kind, &window)?;
                // §7.3：非会话级事件（node 作用域）**没有**会话级 origin cursor，三列（`session_id`/
                // `origin_epoch`/`origin_sequence`）一律 NULL，只分配 `global_sequence`；伪造一个 origin
                // 会被成对 CHECK 拒绝，也违反合同。
                let (session_value, epoch_value, origin_value, session_seq_value) =
                    match &session_id {
                        Some(session) => {
                            let epoch = origin_epoch.clone().ok_or(PortError::InvalidRequest(
                                "a session-scoped event requires an origin epoch",
                            ))?;
                            (
                                Some(session.as_str().to_owned()),
                                Some(epoch.as_str().to_owned()),
                                Some(origin_sequence),
                                Some(session_sequence),
                            )
                        }
                        None => (None, None, None, None),
                    };
                // §3.4：`PendingEvent.origin` 是权威来源（broker 用 `broker::event_origin` 判定），
                // 不再由「有没有会话」推断。`as_str()` 的取值与 §7.3 的 `origin_kind` CHECK 一致。
                let origin_kind = pending.origin;
                let (media_type, raw_json, raw_bytes, raw_sha256, raw_reason) =
                    acp_columns(pending.payload.acp.as_ref())?;
                sqlx::query(INSERT_EVENT)
                    .bind(storable(global)?)
                    .bind(session_value.as_deref())
                    .bind(session_seq_value.map(storable).transpose()?)
                    .bind(&epoch_value)
                    .bind(origin_value.map(storable).transpose()?)
                    .bind(&event_id_text)
                    .bind(pending.turn.as_ref().map(TurnId::as_str))
                    .bind(pending.event_type.as_str())
                    .bind(pending.kind.as_str())
                    .bind(pending.policy.as_str())
                    .bind(origin_kind.as_str())
                    .bind(pending.causation.as_ref().map(RequestId::as_str))
                    .bind(pending.payload.view.as_str())
                    .bind(payload_digest.as_str())
                    .bind(media_type)
                    .bind(raw_json)
                    .bind(raw_bytes)
                    .bind(raw_sha256)
                    .bind(raw_reason)
                    .bind(commit.at.as_str())
                    .bind(expires.as_str())
                    .execute(&mut *tx)
                    .await
                    .db()?;
                let event_id = EventId::new(&event_id_text)
                    .map_err(|_| PortError::InvalidRequest("generated event id is malformed"))?;
                if pending.kind == EventKind::Summary {
                    summary_sequence = Some(sequence_of(global)?);
                }
                if terminal_request.as_ref() == pending.causation.as_ref()
                    && TERMINAL_EVENT_TYPES.contains(&pending.event_type.as_str())
                {
                    terminal_event_id = Some(event_id.clone());
                }
                appended.push(CommittedEvent {
                    event_type: pending.event_type.clone(),
                    id: event_id,
                    session: session_id.clone(),
                    session_sequence: session_seq_value.map(sequence_of).transpose()?,
                    global_sequence: sequence_of(global)?,
                    origin_epoch: epoch_value
                        .as_deref()
                        .map(|value| {
                            OriginEpoch::new(value)
                                .map_err(|_| PortError::InvalidRequest("origin epoch is malformed"))
                        })
                        .transpose()?,
                    origin_sequence: origin_value.map(sequence_of).transpose()?,
                    created_at: commit.at.clone(),
                });
                global += 1;
                if session_id.is_some() {
                    session_sequence += 1;
                    origin_sequence += 1;
                }
            }
        }

        // ---- 消息正文（§6 第 14 条、§7.6）：与事件同一事务折叠进 `owned_message`/`owned_media_ref`。
        // 先落盘正文再允许广播（broker 在 `commit` 返回后才 publish），压缩只影响 delta 事件，
        // 不删除这里已保存的正文。
        if let Some(session) = &session_id {
            materialize_messages(&mut tx, session, &commit.events, &commit.at).await?;
        }

        // ---- §6 第 15 条：把被替代的 delta 行指向那条 summary 事件。
        //
        // 条件更新的 `WHERE` 同时校验「同一会话 / 行存在 / `kind = 'delta'` / `compacted_into IS NULL`」：
        // 任何一条不满足都得不到行，于是整事务 `InvalidRequest`、零写入。`compacted` 为空时不碰任何行。
        if !commit.compacted.is_empty() {
            let target = summary_sequence.ok_or(PortError::InvalidRequest(
                "a compaction commit must carry exactly one summary event",
            ))?;
            let session = commit.session.clone().ok_or(PortError::InvalidRequest(
                "a compaction commit requires a session id",
            ))?;
            for cursor in &commit.compacted {
                if cursor.server_epoch != self.server_epoch()? {
                    return Err(PortError::InvalidRequest(
                        "a compacted cursor belongs to another server epoch",
                    ));
                }
                let affected = sqlx::query(
                    "UPDATE owned_event SET compacted_into = ?1 WHERE global_sequence = ?2 \
                     AND session_id = ?3 AND kind = 'delta' AND compacted_into IS NULL",
                )
                .bind(storable(target.get())?)
                .bind(storable(cursor.global_sequence.get())?)
                .bind(session.as_str())
                .execute(&mut *tx)
                .await
                .db()?
                .rows_affected();
                if affected == 0 {
                    return Err(PortError::InvalidRequest(
                        "a compacted cursor must reference a delta row of this session that is \
                         not compacted yet",
                    ));
                }
            }
        }

        // ---- pending 交互行（§5.2 的 `PendingInteractionWrite`）：与事件/命令同一事务。
        for write in &commit.interactions {
            let session = session_id.as_ref().ok_or(PortError::InvalidRequest(
                "an interaction requires a session id",
            ))?;
            if write.interaction.session() != session {
                return Err(PortError::InvalidRequest(
                    "the interaction must belong to the committed session",
                ));
            }
            // `owned_interaction.request_event` 是 `owned_event.global_sequence` 的外键（列在 STRICT 表里
            // 是 INTEGER，只有行号能入库）。
            //
            // §6 第 13 条：配对键是「同一提交里 `kind = 'interaction'` 且 `payload.view` 的
            // `$.interactionId` 等于本交互 id」的那条事件（permission 与 elicitation 的视图都带该键）。
            // `PendingInteractionWrite` 不再有 `request_event` 字段——`event_id` 由存储层在事务内分配，
            // 装配方无从预知。配不到（事件缺失、被 Ephemeral 过滤、或 `interactionId` 缺失/null）就整事务
            // 拒绝，绝不写悬空引用。
            let paired: Option<i64> = sqlx::query_scalar(
                "SELECT global_sequence FROM owned_event \
                 WHERE kind = 'interaction' \
                 AND json_extract(payload_json, '$.interactionId') = ?1 \
                 ORDER BY global_sequence DESC LIMIT 1",
            )
            .bind(write.interaction.id().as_str())
            .fetch_optional(&mut *tx)
            .await
            .db()?;
            let event_sequence = match paired {
                Some(value) => sequence(value, "owned_event.global_sequence")?,
                None => {
                    return Err(PortError::InvalidRequest(
                        "the interaction has no paired interaction event in this commit",
                    ));
                }
            };
            sqlx::query(
                "INSERT INTO owned_interaction (interaction_id, session_id, kind, request_event, \
                 state, created_at, turn_id) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6)",
            )
            .bind(write.interaction.id().as_str())
            .bind(session.as_str())
            .bind(write.interaction.kind().as_str())
            .bind(storable(event_sequence.get())?)
            .bind(write.interaction.created_at().as_str())
            .bind(write.turn.as_ref().map(TurnId::as_str))
            .execute(&mut *tx)
            .await
            .db()?;
        }

        // ---- 命令（幂等行 + 终态；§7.3 的 CHECK 形状由 `CommandRecord::try_new` 先行校验）
        if let Some(idem) = commit.idempotency.as_ref() {
            // §6 第 20 条：会话级命令的幂等行落**目标会话**；`session.create` 的目标会话是本次事务刚分配的
            // 那一个（装配方预知不了 id），因此这里用 `session_id` 回填它的 `None`——否则终态块与启动恢复
            // 都无法按 `(session, requestId)` 定位该行。
            let idem = match (idem.session.as_ref(), session_id.as_ref()) {
                (None, Some(created)) => acp_core::ports::IdempotencyRecord {
                    session: Some(created.clone()),
                    ..idem.clone()
                },
                _ => idem.clone(),
            };
            write_command(&mut tx, &idem, commit.command_terminal.as_ref()).await?;
        }
        if let Some(request) = &terminal_request {
            let terminal = commit
                .command_terminal
                .as_ref()
                .ok_or(PortError::InvalidRequest(
                    "a command terminal record is required",
                ))?;
            let event = terminal_event_id.as_ref().ok_or(PortError::InvalidRequest(
                "the terminal chunk must contain its command terminal event",
            ))?;
            // §6 第 20 条：条件更新按 `(session_id IS ?, request_id, status = 'accepted')` 定位——
            // `session_id` 为 `None` 时命中的是「尚无会话」的受理行（`session.create` 的失败终态与
            // 启动恢复；`SettleCreate` 的目标行在上面已经回填过会话标识）。
            apply_terminal(
                &mut tx,
                session_id.as_ref(),
                request,
                terminal,
                event,
                terminal_actor,
            )
            .await?;
        }

        // ---- §7.5 ⑥：容量超限时先按 ①②③ 清理，仍超限则拒绝本次写入（事务回滚，不发布事件）。
        enforce_capacity(
            &mut tx,
            &window,
            &commit.at,
            session_id.as_ref(),
            &self.measure_sql,
        )
        .await?;

        tx.commit().await.db()?;
        Ok(CommitOutcome {
            session_id,
            origin_epoch,
            version,
            appended,
            replayed: None,
        })
    }
}

fn mode_columns(mode: &ModeChange) -> (Option<String>, Option<String>) {
    match mode {
        ModeChange::Unchanged => (None, None),
        ModeChange::Set(mode) => (
            Some(mode.mode_id().as_str().to_owned()),
            Some(mode.display_name().to_owned()),
        ),
    }
}

/// §7.3 的 ACP raw 列：`(媒体类型, 原文, 字节数, 摘要, 不可用原因)`。
type AcpColumns = (
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<&'static str>,
);

fn acp_columns(acp: Option<&AcpRaw>) -> Result<AcpColumns, StorageError> {
    match acp {
        None => Ok((None, None, None, None, None)),
        Some(AcpRaw::Available {
            media_type,
            raw_json,
            byte_length,
            sha256,
        }) => Ok((
            Some(media_type.clone()),
            Some(raw_json.clone()),
            Some(
                i64::try_from(*byte_length)
                    .map_err(|_| StorageError::OutOfRange("ACP raw length exceeds i64"))?,
            ),
            Some(sha256.as_str().to_owned()),
            None,
        )),
        Some(AcpRaw::Unavailable {
            reason,
            byte_length,
            sha256,
        }) => Ok((
            None,
            None,
            Some(
                i64::try_from(*byte_length)
                    .map_err(|_| StorageError::OutOfRange("ACP raw length exceeds i64"))?,
            ),
            // §7.3 的 CHECK 明确为「不可用原因行」开了口子：摘要算得出来而原文已被保留期/容量清掉是
            // 合法状态，因此摘要照写（丢摘要等于丢保真信息）。
            sha256.as_ref().map(|digest| digest.as_str().to_owned()),
            Some(reason.as_str()),
        )),
    }
}

async fn insert_turn(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    session: &SessionId,
    new_turn: &NewTurn,
) -> Result<(), PortError> {
    let queue_index: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(queue_index), 0) + 1 FROM owned_turn WHERE session_id = ?1",
    )
    .bind(session.as_str())
    .fetch_one(&mut **tx)
    .await
    .db()?;
    sqlx::query(
        "INSERT INTO owned_turn (turn_id, session_id, state, queue_index, causation, started_at, \
         ended_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
    )
    .bind(new_turn.turn.as_str())
    .bind(session.as_str())
    .bind(new_turn.state.as_str())
    .bind(queue_index)
    .bind(new_turn.causation.as_ref().map(RequestId::as_str))
    .bind(new_turn.started_at.as_ref().map(Timestamp::as_str))
    .execute(&mut **tx)
    .await
    .db()?;
    Ok(())
}

async fn update_turn(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    session: &SessionId,
    update: &TurnUpdate,
) -> Result<(), PortError> {
    let affected = sqlx::query(
        "UPDATE owned_turn SET state = ?1, ended_at = ?2 WHERE turn_id = ?3 AND session_id = ?4",
    )
    .bind(update.state.as_str())
    .bind(update.ended_at.as_ref().map(Timestamp::as_str))
    .bind(update.turn.as_str())
    .bind(session.as_str())
    .execute(&mut **tx)
    .await
    .db()?
    .rows_affected();
    if affected == 0 {
        return Err(PortError::NotFound(EntityRef::Turn(update.turn.clone())));
    }
    Ok(())
}

async fn write_command(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    idem: &acp_core::ports::IdempotencyRecord,
    terminal: Option<&acp_core::model::CommandTerminalRecord>,
) -> Result<(), PortError> {
    let status = terminal.map_or(CommandStatus::Accepted, |value| value.status());
    // 先按 §7.3 的 CHECK 形状构造一次：形状不合法就地拒绝，不让 sqlite 报约束错误。
    let record = CommandRecord::try_new(
        idem.session.clone(),
        idem.request.clone(),
        &idem.command,
        idem.kind,
        idem.actor.clone(),
        Some(idem.accepted_at.clone()),
        status,
        terminal.and_then(|value| value.terminal_at().cloned()),
        terminal.and_then(|value| value.terminal_event().cloned()),
        terminal.and_then(|value| value.result().cloned()),
        terminal.and_then(|value| value.error().cloned()),
        idem.expected_version,
        idem.request_fingerprint.clone(),
    )
    .map_err(|_| PortError::InvalidRequest("command record violates §7.3 invariants"))?;
    let (actor_kind, actor_id) = actor_key(record.actor());
    let (error_code, error_message, error_details, retryable) = error_columns(record.error());
    let affected = sqlx::query(INSERT_COMMAND)
        .bind(actor_kind.as_str())
        .bind(actor_id)
        .bind(record.request().as_str())
        .bind(record.session().map(SessionId::as_str))
        .bind(record.command())
        .bind(record.kind().as_str())
        .bind(
            record
                .expected_version()
                .map(|value| storable(value.get()))
                .transpose()?,
        )
        .bind(record.request_fingerprint().as_str())
        .bind(record.accepted_at().map(Timestamp::as_str))
        .bind(record.status().as_str())
        .bind(record.terminal_at().map(Timestamp::as_str))
        .bind(record.terminal_event().map(EventId::as_str))
        .bind(record.result().map(|value| value.as_str()))
        .bind(error_code)
        .bind(error_message)
        .bind(error_details)
        .bind(retryable)
        .execute(&mut **tx)
        .await
        .db()?
        .rows_affected();
    if affected == 0 {
        return Err(PortError::InvalidRequest("command row was not written"));
    }
    Ok(())
}

/// §10.1 的命令终态事件类型（`SYNC_PROTOCOL.md` §10.1）：终态块靠它们定位既有命令行。
const TERMINAL_EVENT_TYPES: [&str; 3] =
    ["command.completed", "command.failed", "command.uncertain"];

/// 终态块里承载 requestId 的那个事件：`causation` 即被推进到终态的命令。
fn terminal_request_of(events: &[acp_core::model::PendingEvent]) -> Option<RequestId> {
    events
        .iter()
        .find(|event| TERMINAL_EVENT_TYPES.contains(&event.event_type.as_str()))
        .and_then(|event| event.causation.clone())
}

/// §6 第 20 条：按 `(session_id, request_id)` 定位既有命令行。
///
/// **不按状态过滤**：已经终结的行必须能被找到并按**重放**回答（同一终态块重复投递不追加事件、不改行），
/// 因此这里只负责定位与「命中多行即失败关闭」（同一 `requestId` 在多条 actor 下各有一行，不猜是哪一条）。
async fn locate_session_command(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    session: &SessionId,
    request: &RequestId,
) -> Result<Option<SqliteRow>, PortError> {
    let rows = sqlx::query(&format!(
        "SELECT {COMMAND_COLUMNS} FROM owned_command \
         WHERE session_id = ?1 AND request_id = ?2 LIMIT 2"
    ))
    .bind(session.as_str())
    .bind(request.as_str())
    .fetch_all(&mut **tx)
    .await
    .db()?;
    single_row(rows, "request id is ambiguous within the session")
}

/// §6 第 20 条：定位**尚无会话**的行（`session_id IS NULL`）——`session.create` 的失败终态、启动恢复
/// 与落账提交（回填之前）都走这一条。命中多行即失败关闭（同上：不猜）。
async fn locate_unsessioned_command(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    request: &RequestId,
) -> Result<Option<SqliteRow>, PortError> {
    let rows = sqlx::query(&format!(
        "SELECT {COMMAND_COLUMNS} FROM owned_command \
         WHERE request_id = ?1 AND session_id IS NULL LIMIT 2"
    ))
    .bind(request.as_str())
    .fetch_all(&mut **tx)
    .await
    .db()?;
    single_row(rows, "request id is ambiguous across actors")
}

fn single_row(
    rows: Vec<SqliteRow>,
    ambiguous: &'static str,
) -> Result<Option<SqliteRow>, PortError> {
    let mut rows = rows.into_iter();
    match (rows.next(), rows.next()) {
        (None, _) => Ok(None),
        (Some(row), None) => Ok(Some(row)),
        (Some(_), Some(_)) => Err(PortError::InvalidRequest(ambiguous)),
    }
}

/// §6 第 20 条：落账提交把受理行的 `session_id` 回填成新建的会话（终态提交与启动恢复都按它定位）。
///
/// 条件更新只认「尚无会话且仍是 `accepted`」那一行：回填 0 行说明受理行不在预期形状上（别的路径已经
/// 动过它），整事务失败关闭而不是把终态写到别的行上。
async fn backfill_command_session(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    request: &RequestId,
    session: &SessionId,
) -> Result<(), PortError> {
    let affected = sqlx::query(
        "UPDATE owned_command SET session_id = ?1 \
         WHERE request_id = ?2 AND session_id IS NULL AND status = 'accepted'",
    )
    .bind(session.as_str())
    .bind(request.as_str())
    .execute(&mut **tx)
    .await
    .db()?
    .rows_affected();
    if affected != 1 {
        return Err(PortError::InvalidRequest(
            "the accepted session.create row could not be bound to the created session",
        ));
    }
    Ok(())
}

/// 落账/创建提交要求的 origin epoch。
fn committed_epoch(epoch: &Option<OriginEpoch>) -> Result<OriginEpoch, PortError> {
    epoch.clone().ok_or(PortError::InvalidRequest(
        "session creation requires an origin epoch",
    ))
}

/// 建一行 owned 会话（`state = 'idle'`、`version = 1`、两个时间戳都取本次提交的 `at`）。
async fn insert_session_row(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    id_text: &str,
    title: &Option<String>,
    agent: &acp_core::model::AgentRef,
    epoch: &OriginEpoch,
    at: &Timestamp,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, \
         origin_epoch, current_mode_id, current_mode_name, version, created_at, \
         updated_at) \
         VALUES (?1, ?2, ?3, ?4, 'idle', ?5, NULL, NULL, 1, ?6, ?6)",
    )
    .bind(id_text)
    .bind(title.as_deref())
    .bind(agent.agent_id().as_str())
    .bind(agent.name())
    .bind(epoch.as_str())
    .bind(at.as_str())
    .execute(&mut **tx)
    .await
    .db()?;
    Ok(())
}

/// `session.create` 的落账建行（§6 第 20 条）：标识由 core 分配、存储层采用，ACP 会话标识 / 创建时
/// cwd / 目录别名与它同一行落盘（三条 `CASE`-窄写的旧路径不再参与创建）。
async fn insert_created_session_row(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    settled: &SettledSession,
    epoch: &OriginEpoch,
    at: &Timestamp,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, \
         origin_epoch, current_mode_id, current_mode_name, version, created_at, updated_at, \
         agent_session_id, workspace_cwd, workspace_directory_id) \
         VALUES (?1, ?2, ?3, ?4, 'idle', ?5, NULL, NULL, 1, ?6, ?6, ?7, ?8, ?9)",
    )
    .bind(settled.session.as_str())
    .bind(settled.title.as_deref())
    .bind(settled.agent.agent_id().as_str())
    .bind(settled.agent.name())
    .bind(epoch.as_str())
    .bind(at.as_str())
    .bind(
        settled
            .agent_session_id
            .as_ref()
            .map(AgentSessionId::as_str),
    )
    .bind(settled.workspace_cwd.as_deref())
    .bind(settled.workspace_directory_id.as_deref())
    .execute(&mut **tx)
    .await
    .db()?;
    // 目录记录与成功的会话在**同一次提交**里写入（`design.md` D2）：`INSERT ... ON CONFLICT DO NOTHING`
    // 让并发创建同一目录时幂等，且不改写既有的 `created_at`。
    if let (Some(directory_id), Some(cwd)) =
        (&settled.workspace_directory_id, &settled.workspace_cwd)
    {
        sqlx::query(
            "INSERT INTO owned_directory (directory_id, canonical_path, created_at, last_used_at) \
             VALUES (?1, ?2, ?3, ?3) ON CONFLICT(directory_id) DO UPDATE SET \
             last_used_at = excluded.last_used_at",
        )
        .bind(directory_id)
        .bind(cwd)
        .bind(at.as_str())
        .execute(&mut **tx)
        .await
        .db()?;
    }
    Ok(())
}

/// 把既有 `accepted` 行推进到终态（§11.2）。
///
/// `terminal_at`/`result`/`error` 为 `None` 表示**保持既有列不变**：终态块无法重发接受时的结果，
/// 用 `COALESCE` 而不是覆盖，避免把接受期写入的列清空。
///
/// `session` 为 `None` 时按 `session_id IS NULL` 定位（§6 第 20 条的「尚无会话」形状：`session.create`
/// 的失败终态与启动恢复）。
async fn apply_terminal(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    session: Option<&SessionId>,
    request: &RequestId,
    terminal: &acp_core::model::CommandTerminalRecord,
    terminal_event: &EventId,
    terminal_actor: Option<&Actor>,
) -> Result<(), PortError> {
    let (error_code, error_message, error_details, retryable) = error_columns(terminal.error());
    let scoped_actor = terminal_actor.map(actor_key);
    // `session_id IS ?9`：`IS` 与 `=` 的唯一区别就是 NULL 参与比较时的取值（`NULL IS NULL` 为真），
    // 因此同一个条件能同时表达「按会话定位」与「按尚无会话定位」，不必写两条语句。
    let affected = sqlx::query(
        "UPDATE owned_command SET status = ?1, \
         terminal_at = COALESCE(?2, terminal_at), terminal_event_id = ?3, \
         result_json = COALESCE(?4, result_json), \
         error_code = COALESCE(?5, error_code), \
         error_message = COALESCE(?6, error_message), \
         error_details_json = COALESCE(?7, error_details_json), \
         retryable = COALESCE(?8, retryable) \
         WHERE session_id IS ?9 AND request_id = ?10 AND status = 'accepted' \
         AND (?11 IS NULL OR (actor_kind = ?11 AND actor_id = ?12))",
    )
    .bind(terminal.status().as_str())
    .bind(terminal.terminal_at().map(Timestamp::as_str))
    .bind(terminal_event.as_str())
    .bind(terminal.result().map(|value| value.as_str()))
    .bind(error_code)
    .bind(error_message)
    .bind(error_details)
    .bind(retryable)
    .bind(session.map(SessionId::as_str))
    .bind(request.as_str())
    .bind(scoped_actor.as_ref().map(|(kind, _)| kind.as_str()))
    .bind(scoped_actor.as_ref().map(|(_, id)| id.as_str()))
    .execute(&mut **tx)
    .await
    .db()?
    .rows_affected();
    if affected == 0 {
        // 行在本次事务开始前被读为 `accepted`，写锁由本事务持有；走到这里说明状态与读取时不一致。
        return Err(PortError::Conflict(ConflictKind::Consumed));
    }
    Ok(())
}

/// §5.2/§6.7：交互解析的 first-writer-wins。
///
/// 条件更新（`state = 'pending'` 且未失效、版本相符）+ 受影响行数为 0 时**在同一事务内**回读：
/// 行存在且 `state <> 'pending'` → `Conflict(AlreadyResolved)`；无行或跨会话 → `NotFound`；
/// 版本不符 → `Conflict(VersionMismatch)`；已失效 → `Conflict(Expired)`。
/// 因此第二个应答不会覆盖既有结果（`decision_*`/`resolved_at` 保持不变）。仲裁**只**写
/// `resolved_at`，`confirmed_at` 保持 `NULL`：处理结果确认之前该行仍计入有效待处理。
async fn resolve_interaction(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    session: &SessionId,
    resolved: &acp_core::ports::InteractionResolved,
    at: &Timestamp,
) -> Result<(), PortError> {
    let (decision_option_id, decision_kind, elicitation_action, elicitation_values) =
        match &resolved.resolution {
            InteractionResolution::Permission(decision) => (
                Some(decision.option_id().to_owned()),
                Some(decision.kind().as_str()),
                None,
                None,
            ),
            InteractionResolution::Elicitation { action, values } => (
                None,
                None,
                Some(action.as_str()),
                Some(values.to_json_text()),
            ),
        };
    let (resolved_by_kind, resolved_by_id) = actor_key(&resolved.resolved_by);
    let expected_version = resolved
        .request_version
        .map(|version| storable(version.get()))
        .transpose()?;
    let affected = sqlx::query(
        "UPDATE owned_interaction SET state = 'resolved', resolved_at = ?1, \
         decision_option_id = ?2, decision_kind = ?3, elicitation_action = ?4, \
         elicitation_values_json = ?5, resolved_by_kind = ?6, resolved_by_id = ?7 \
         WHERE interaction_id = ?8 AND session_id = ?9 AND state = 'pending' \
         AND invalidated_at IS NULL AND (?10 IS NULL OR request_version = ?10)",
    )
    .bind(at.as_str())
    .bind(decision_option_id)
    .bind(decision_kind)
    .bind(elicitation_action)
    .bind(elicitation_values)
    .bind(resolved_by_kind.as_str())
    .bind(resolved_by_id)
    .bind(resolved.interaction.as_str())
    .bind(session.as_str())
    .bind(expected_version)
    .execute(&mut **tx)
    .await
    .db()?
    .rows_affected();
    if affected > 0 {
        return Ok(());
    }
    let row = sqlx::query(
        "SELECT state, invalidated_at, request_version, session_id FROM owned_interaction \
         WHERE interaction_id = ?1",
    )
    .bind(resolved.interaction.as_str())
    .fetch_optional(&mut **tx)
    .await
    .db()?;
    let Some(row) = row else {
        return Err(PortError::NotFound(EntityRef::Interaction(
            resolved.interaction.clone(),
        )));
    };
    if text(&row, "session_id")? != session.as_str() {
        // 跨会话请求按「不存在」处理，避免泄露其它会话的交互事实。
        return Err(PortError::NotFound(EntityRef::Interaction(
            resolved.interaction.clone(),
        )));
    }
    if opt_text(&row, "invalidated_at")?.is_some() {
        // 失效请求不可被旧路径消费：与 `invalidate_pending` 同一口径（Expired，而不是
        // 「已处理」），调用方因此不会把「上游已丢失」误读成「别人先答了」。
        return Err(PortError::Conflict(ConflictKind::Expired));
    }
    if text(&row, "state")? != "pending" {
        return Err(PortError::Conflict(ConflictKind::AlreadyResolved));
    }
    if let Some(expected) = expected_version {
        if int(&row, "request_version")? != expected {
            return Err(PortError::Conflict(ConflictKind::VersionMismatch));
        }
    }
    Err(PortError::Conflict(ConflictKind::AlreadyResolved))
}

/// 一次应答**确认**：把 `resolved` 且未确认的行写入 `confirmed_at`。
///
/// 确认是交互退出有效集合的唯一途径，但它不是「仲裁」的替代：未知行、仍 `pending`、已失效或已确认
/// 的行都是**无操作**（重复投递/旧 peer 不产生第二次副作用，也不报错）。
async fn confirm_interaction(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    session: &SessionId,
    interaction: &InteractionId,
    at: &Timestamp,
) -> Result<(), PortError> {
    sqlx::query(
        "UPDATE owned_interaction SET confirmed_at = ?1 \
         WHERE interaction_id = ?2 AND session_id = ?3 AND state = 'resolved' \
         AND confirmed_at IS NULL",
    )
    .bind(at.as_str())
    .bind(interaction.as_str())
    .bind(session.as_str())
    .execute(&mut **tx)
    .await
    .db()?;
    Ok(())
}

/// turn 进入终态时失效它名下仍未解决的交互（`pending` 或已仲裁未确认）。
///
/// `invalidated_at` 与 `state = 'expired'` 同时写：所有「仍在等待」的读面都按 `state = 'pending'`
/// 过滤，只写标记会让失效请求继续出现在读面里；`confirmed_at` 不动，行仍保留原有仲裁结果供审计。
async fn expire_turn_interactions(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    turn: &TurnId,
    at: &Timestamp,
) -> Result<(), PortError> {
    sqlx::query(
        "UPDATE owned_interaction SET state = 'expired', invalidated_at = ?1 \
         WHERE turn_id = ?2 AND invalidated_at IS NULL \
         AND (state = 'pending' OR (state = 'resolved' AND confirmed_at IS NULL))",
    )
    .bind(at.as_str())
    .bind(turn.as_str())
    .execute(&mut **tx)
    .await
    .db()?;
    Ok(())
}

fn error_columns(
    error: Option<&PublicError>,
) -> (Option<String>, Option<String>, Option<String>, Option<i64>) {
    match error {
        None => (None, None, None, None),
        Some(error) => (
            Some(error.code().to_owned()),
            Some(error.message().to_owned()),
            Some(error.details().as_str().to_owned()),
            Some(i64::from(error.retryable())),
        ),
    }
}
// ---------------------------------------------------------------------------------------------
// §7.5 保留、清理与容量
// ---------------------------------------------------------------------------------------------

/// 一次清理批次的结果（用于累加 [`PruneReport`]）。
#[derive(Debug, Clone, Copy, Default)]
struct Sweep {
    events: u64,
    interactions: u64,
    attachments: u64,
    audit: u64,
    messages: u64,
    freed_bytes: u64,
}

impl Sweep {
    fn add(&mut self, other: Sweep) {
        self.events += other.events;
        self.interactions += other.interactions;
        self.attachments += other.attachments;
        self.audit += other.audit;
        self.messages += other.messages;
        self.freed_bytes += other.freed_bytes;
    }
}

/// ① / ②：按 `expires_at` 清理一类事件，返回 (行数, 释放字节)。
async fn sweep_events(
    ex: &mut SqliteConnection,
    predicate: &str,
    at: &Timestamp,
) -> Result<Sweep, StorageError> {
    let measure = measure_events_sql(predicate);
    let (rows, bytes): (i64, i64) = sqlx::query_as(&measure)
        .bind(at.as_str())
        .fetch_one(&mut *ex)
        .await
        .db()?;
    if rows == 0 {
        return Ok(Sweep::default());
    }
    let delete = delete_events_sql(predicate);
    let deleted = sqlx::query(&delete)
        .bind(at.as_str())
        .execute(&mut *ex)
        .await
        .db()?
        .rows_affected();
    Ok(Sweep {
        events: deleted,
        freed_bytes: u64::try_from(bytes).unwrap_or(0),
        ..Sweep::default()
    })
}

/// ③：删除指定会话（或全库）最旧的**已压缩** delta 批次。
///
/// §7.5 只在超限时触发容量清理；这里按 `global_sequence` 升序分批删除 `compacted_into IS NOT NULL`
/// 的行，直到不再超限或已无行可删。分批（64 行）是为了在有并发读的情况下不长时间持有写事务。
async fn sweep_compacted(
    ex: &mut SqliteConnection,
    limits: &RetentionPolicy,
    session: Option<&SessionId>,
    measure_sql: &str,
) -> Result<Sweep, StorageError> {
    let predicate = match session {
        Some(_) => "compacted_into IS NOT NULL AND session_id = ?1",
        None => "compacted_into IS NOT NULL",
    };
    let mut sweep = Sweep::default();
    while over_capacity(ex, limits, session, measure_sql).await? {
        let sql = format!(
            "SELECT rowid AS row_id, COALESCE(length(payload_json), 0) + COALESCE(length(acp_raw_json), 0) AS bytes \
             FROM owned_event WHERE {predicate} ORDER BY global_sequence ASC LIMIT 64"
        );
        let mut query = sqlx::query(&sql);
        if let Some(session) = session {
            query = query.bind(session.as_str());
        }
        let rows = query.fetch_all(&mut *ex).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let row_id = int(&row, "row_id")?;
            let bytes = opt_int(&row, "bytes")?.unwrap_or(0);
            let affected = sqlx::query("DELETE FROM owned_event WHERE rowid = ?1")
                .bind(row_id)
                .execute(&mut *ex)
                .await
                .db()?
                .rows_affected();
            sweep.events += affected;
            sweep.freed_bytes += u64::try_from(bytes).unwrap_or(0);
        }
    }
    Ok(sweep)
}

/// 是否仍然超限（全库上限或该会话上限）。
async fn over_capacity(
    ex: &mut SqliteConnection,
    limits: &RetentionPolicy,
    session: Option<&SessionId>,
    measure_sql: &str,
) -> Result<bool, StorageError> {
    if measure_total(ex, measure_sql).await? > limits.max_total_size_bytes {
        return Ok(true);
    }
    if let Some(session) = session
        && measure_session(ex, session).await? > limits.max_session_size_bytes
    {
        return Ok(true);
    }
    Ok(false)
}

/// 容量清理的 ①/② 谓词。
///
/// 两个比较**故意用同一个参数 `?1`**（`sweep_events` 只绑定一次）：`created_at < ?1` 中的严格小于
/// 排除了**本次提交**刚写入的事件（它们的 `created_at` 正是 `commit.at`），否则一次提交可能在同一事务里
/// 删掉自己刚写的事件。绑定次数必须与谓词里的占位符数量一致——写成 `?2` 而只绑定一次会让
/// `created_at < NULL` 恒为 NULL，① ② 静默变成空操作。
/// ②′：引用的事件已过期的**终态**交互行。必须先于 ①② 执行：`foreign_keys = ON` 下删掉仍被引用的
/// 事件会抛 `FOREIGN KEY constraint failed`，进而把 prune / commit 的整个事务拖回滚（§7.5）。
const TERMINAL_INTERACTION_PREDICATE: &str = "state <> 'pending' AND request_event IS NOT NULL AND request_event IN \
     (SELECT global_sequence FROM owned_event WHERE expires_at IS NOT NULL AND expires_at <= ?1)";

const CAPACITY_DELTA_PREDICATE: &str = "kind = 'delta' AND expires_at IS NOT NULL AND expires_at <= ?1 AND created_at < ?1 \
                 AND NOT EXISTS (SELECT 1 FROM owned_interaction i \
                 WHERE i.request_event = owned_event.global_sequence)";
const CAPACITY_RETAINED_PREDICATE: &str = "kind <> 'delta' AND expires_at IS NOT NULL AND expires_at <= ?1 AND created_at < ?1 \
                 AND NOT EXISTS (SELECT 1 FROM owned_interaction i \
                 WHERE i.request_event = owned_event.global_sequence)";

/// §7.5 ⑥：容量检查。超限时按 ①（过期 delta）→ ②（过期正文/状态）→ ③（已压缩批次）清理；
/// 仍超限则拒绝本次写入。管理写集经 `admin::enforce_capacity_gate` 调用的就是本函数，两条路径
/// 共用同一份顺序与判据（§11.2 第 7 条的「管理写集纳入容量」）。
///
/// §7.5 的顺序里 ④（附件 LRU）排在 ③ 之后，但附件字节在文件系统上：在本事务内删行而事务可能回滚，
/// 会造成「行还在、文件已删」的悬空行。因此容量清理在写事务内只做数据库侧（③①②），附件 LRU 由
/// `prune` / `AttachmentStore::prune_lru` 在事务提交后执行。若附件本身就把库顶到上限之上，本次提交会
/// 明确返回 `Unavailable(StorageFull)`，而不是静默丢弃附件或正文。
pub(crate) async fn enforce_capacity(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    limits: &RetentionPolicy,
    at: &Timestamp,
    session: Option<&SessionId>,
    measure_sql: &str,
) -> Result<(), PortError> {
    if !over_capacity(tx, limits, session, measure_sql).await? {
        return Ok(());
    }
    // ①② 先清已过窗口的事件：delta 先于正文/结构化/状态（§7.5 的清理顺序 ① → ②）。
    // ②′ 终态交互行：与 prune 同一顺序，先解外键再清事件。
    sqlx::query(&format!(
        "DELETE FROM owned_interaction WHERE {TERMINAL_INTERACTION_PREDICATE}"
    ))
    .bind(at.as_str())
    .execute(&mut **tx)
    .await
    .db()?;
    sweep_events(tx, CAPACITY_DELTA_PREDICATE, at).await?;
    // §7.5 是「清理到不再超限」：① 足够时不动 ②（② 删的是承诺期更长的行）。
    if over_capacity(tx, limits, session, measure_sql).await? {
        sweep_events(tx, CAPACITY_RETAINED_PREDICATE, at).await?;
    }
    // ③ 该会话最旧的已压缩 delta 批次，再退到全库。
    if over_capacity(tx, limits, session, measure_sql).await? {
        if let Some(session) = session {
            sweep_compacted(tx, limits, Some(session), measure_sql).await?;
        }
        sweep_compacted(tx, limits, None, measure_sql).await?;
    }
    if over_capacity(tx, limits, session, measure_sql).await? {
        return Err(PortError::Unavailable(UnavailableKind::StorageFull));
    }
    Ok(())
}

/// ④：按 `last_used_at` LRU 删除附件，直到总字节数降到预算以内。
///
/// 返回 `(被删除的 sha256, 释放字节, 被删除的文件路径)`；文件删除由调用方在事务提交后执行
/// （§10 把「附件文件删除的事务边界」列为待决项，这里采取保守顺序：先提交行删除，再删文件，
/// 崩溃残留的是无人引用的孤儿文件而不是悬空行）。
async fn sweep_attachments(
    ex: &mut SqliteConnection,
    budget_bytes: u64,
    at: &Timestamp,
) -> Result<(u64, u64, Vec<String>), StorageError> {
    let total: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(byte_length), 0) FROM owned_attachment")
            .fetch_one(&mut *ex)
            .await
            .db()?;
    let mut remaining = u64::try_from(total).unwrap_or(0);
    if remaining <= budget_bytes {
        return Ok((0, 0, Vec::new()));
    }
    let candidates: Vec<SqliteRow> = sqlx::query(
        "SELECT sha256, byte_length, relative_path FROM owned_attachment a \
         WHERE NOT EXISTS (SELECT 1 FROM owned_media_ref r WHERE r.attachment_id = a.attachment_id) \
           AND NOT EXISTS (SELECT 1 FROM owned_attachment_submission_item i \
                           WHERE i.attachment_id = a.attachment_id) \
         ORDER BY last_used_at ASC, sha256 ASC",
    )
    .fetch_all(&mut *ex)
    .await
    .db()?;
    let mut removed = 0_u64;
    let mut freed = 0_u64;
    let mut files = Vec::new();
    let mut digests = Vec::new();
    for row in candidates {
        if remaining <= budget_bytes {
            break;
        }
        let sha = text(&row, "sha256")?;
        let bytes = u64::try_from(int(&row, "byte_length")?).unwrap_or(0);
        let relative = text(&row, "relative_path")?;
        // 先删引用（`owned_attachment_link.sha256` 没有 ON DELETE，必须先删链接），再删元数据。
        sqlx::query("DELETE FROM owned_attachment_link WHERE sha256 = ?1")
            .bind(&sha)
            .execute(&mut *ex)
            .await
            .db()?;
        let affected = sqlx::query("DELETE FROM owned_attachment WHERE sha256 = ?1")
            .bind(&sha)
            .execute(&mut *ex)
            .await
            .db()?
            .rows_affected();
        removed += affected;
        freed += bytes;
        remaining = remaining.saturating_sub(bytes);
        digests.push(sha);
        files.push(relative);
    }
    crate::transfers::expire_transfers_for_digests(ex, at, &digests).await?;
    Ok((removed, freed, files))
}

/// 附件 GC（按引用）：删除既无会话绑定、又无消息媒体引用、也不在任何提交清单里，且超过 24 小时
/// 未被使用的附件。共享引用因此天然安全：只要还有一条保留消息引用它，内容就不会被回收。
///
/// 返回 `(行数, 释放字节, 文件路径)`；文件由调用方在事务提交后删除。
async fn sweep_unreferenced_attachments(
    ex: &mut SqliteConnection,
    cutoff: &Timestamp,
    at: &Timestamp,
) -> Result<(u64, u64, Vec<String>), StorageError> {
    let rows: Vec<SqliteRow> = sqlx::query(
        "SELECT attachment_id, sha256, byte_length, relative_path FROM owned_attachment a \
         WHERE a.last_used_at <= ?1 \
           AND NOT EXISTS (SELECT 1 FROM owned_attachment_link l \
                           WHERE l.attachment_id = a.attachment_id) \
           AND NOT EXISTS (SELECT 1 FROM owned_media_ref r \
                           WHERE r.attachment_id = a.attachment_id) \
           AND NOT EXISTS (SELECT 1 FROM owned_attachment_submission_item i \
                           WHERE i.attachment_id = a.attachment_id)",
    )
    .bind(cutoff.as_str())
    .fetch_all(&mut *ex)
    .await
    .db()?;
    let mut removed = 0_u64;
    let mut freed = 0_u64;
    let mut files = Vec::new();
    let mut digests = Vec::new();
    for row in rows {
        let id = text(&row, "attachment_id")?;
        let sha = text(&row, "sha256")?;
        let bytes = u64::try_from(int(&row, "byte_length")?).unwrap_or(0);
        // 先删行（引用已由谓词排除，外键不会阻挡），文件在事务提交后删除。
        sqlx::query("DELETE FROM owned_attachment WHERE attachment_id = ?1")
            .bind(&id)
            .execute(&mut *ex)
            .await
            .db()?;
        removed += 1;
        freed += bytes;
        digests.push(sha);
        files.push(text(&row, "relative_path")?);
    }
    // 附件行没了以后，指向它的已封存传输不能继续报 `sealed`（否则 `transfer()` 会得到
    // `sealed: None`，重试 `seal_transfer` 也只会撞上 `Corrupt`）。两列一起改以维持 CHECK。
    crate::transfers::expire_transfers_for_digests(ex, at, &digests).await?;
    Ok((removed, freed, files))
}

// ---------------------------------------------------------------------------------------------
// owned_message / owned_media_ref：事件 → 权威正文
// ---------------------------------------------------------------------------------------------

/// 从事件 view 解析一个对象；正文必须是 JSON object（`PendingEvent::validate` 已保证形状）。
fn message_view(pending: &acp_core::model::PendingEvent) -> Result<serde_json::Value, PortError> {
    serde_json::from_str(pending.payload.view.as_str())
        .map_err(|_| PortError::InvalidRequest("message event view is not json"))
}

fn view_text(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

/// 一个 turn 终态对消息正文的含义。
fn terminal_message_state(
    pending: &acp_core::model::PendingEvent,
) -> Option<(MessageState, Option<PartialReason>)> {
    match pending.event_type.as_str() {
        "turn.completed" => Some((MessageState::Completed, None)),
        "turn.cancelled" => Some((MessageState::Cancelled, Some(PartialReason::TurnCancelled))),
        "turn.failed" => Some((MessageState::Failed, Some(PartialReason::AgentFailed))),
        _ => None,
    }
}

/// 本批次里出现的 turn 终态（最多一个；同一批出现多个说明组装错误）。
type TerminalState = (Option<TurnId>, MessageState, Option<PartialReason>);

fn terminal_of(
    events: &[acp_core::model::PendingEvent],
) -> Result<Option<TerminalState>, PortError> {
    let mut found: Option<TerminalState> = None;
    for pending in events {
        if let Some((state, reason)) = terminal_message_state(pending) {
            // 同一批出现多个终态只可能是组装错误；消息折叠取第一条，事件本身仍按原样落库。
            found.get_or_insert((pending.turn.clone(), state, reason));
        }
    }
    Ok(found)
}

fn media_kind_of(block_type: &str) -> Option<MediaKind> {
    match block_type {
        "image_ref" => Some(MediaKind::Image),
        "audio_ref" => Some(MediaKind::Audio),
        "file_ref" => Some(MediaKind::File),
        _ => None,
    }
}

/// 把本批事件折叠进 `owned_message` / `owned_media_ref`（§6 第 14 条、§7.6）。
///
/// 回复与思考分别持久化；`agent.thought.delta` 不产生 `agent.message.completed`，因此思考消息在 turn
/// 终态时按终态收尾（正常完成 = `completed`，停止/失败保留正文并标记原因）。正文只来自事件 view，
/// 存储层不解析 ACP 原文。
async fn materialize_messages(
    tx: &mut Transaction<'_, Sqlite>,
    session: &SessionId,
    events: &[acp_core::model::PendingEvent],
    at: &Timestamp,
) -> Result<(), PortError> {
    let relevant = events.iter().any(|pending| {
        matches!(
            pending.event_type.as_str(),
            "agent.message.delta"
                | "agent.thought.delta"
                | "agent.message.completed"
                | "turn.completed"
                | "turn.cancelled"
                | "turn.failed"
        )
    });
    if !relevant {
        return Ok(());
    }
    let terminal = terminal_of(events)?;

    // 1. delta：先确保行存在，再按到达顺序追加 content block。
    for pending in events {
        let channel = match pending.event_type.as_str() {
            "agent.message.delta" => MessageChannel::Reply,
            "agent.thought.delta" => MessageChannel::Thought,
            _ => continue,
        };
        let view = message_view(pending)?;
        let Some(message_id) = view_text(&view, "messageId") else {
            // 旧路径/非消息 delta 可能没有 messageId；事件照常落库，正文折叠只服务已类型化的消息。
            continue;
        };
        let Ok(message_id) = MessageId::new(&message_id) else {
            continue;
        };
        sqlx::query(
            "INSERT INTO owned_message (message_id, session_id, turn_id, channel, state, \
             content_json, partial_reason, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'partial', '[]', NULL, ?5, ?5) \
             ON CONFLICT(message_id) DO NOTHING",
        )
        .bind(message_id.as_str())
        .bind(session.as_str())
        .bind(pending.turn.as_ref().map(TurnId::as_str))
        .bind(channel.as_str())
        .bind(at.as_str())
        .execute(&mut **tx)
        .await
        .db()?;
        let row = sqlx::query(
            "SELECT channel, state, content_json FROM owned_message WHERE message_id = ?1",
        )
        .bind(message_id.as_str())
        .fetch_one(&mut **tx)
        .await
        .db()?;
        let stored_channel: MessageChannel = text(&row, "channel")?
            .parse()
            .map_err(|_| PortError::Corrupt("owned_message.channel is not a known channel"))?;
        if stored_channel != channel {
            return Err(PortError::InvalidRequest(
                "a message id cannot change its channel",
            ));
        }
        let state: MessageState = text(&row, "state")?
            .parse()
            .map_err(|_| PortError::Corrupt("owned_message.state is not a known state"))?;
        if state == MessageState::Completed {
            return Err(PortError::InvalidRequest(
                "a message delta arrived after its completion",
            ));
        }
        let mut blocks: Vec<serde_json::Value> = serde_json::from_str(&text(&row, "content_json")?)
            .map_err(|_| PortError::Corrupt("owned_message.content_json is not an array"))?;
        let block = match view.get("block") {
            Some(block) if block.is_object() => block.clone(),
            Some(_) => {
                return Err(PortError::InvalidRequest(
                    "message delta block is not an object",
                ));
            }
            None => {
                let text = view_text(&view, "text")
                    .ok_or(PortError::InvalidRequest("message delta has no text"))?;
                serde_json::json!({ "type": "text", "text": text })
            }
        };
        blocks.push(block);
        let content = serde_json::to_string(&blocks)
            .map_err(|_| PortError::InvalidRequest("message content is not serializable"))?;
        sqlx::query(
            "UPDATE owned_message SET content_json = ?1, updated_at = ?2, \
             turn_id = COALESCE(turn_id, ?3) WHERE message_id = ?4",
        )
        .bind(content)
        .bind(at.as_str())
        .bind(pending.turn.as_ref().map(TurnId::as_str))
        .bind(message_id.as_str())
        .execute(&mut **tx)
        .await
        .db()?;
    }

    // 2. `agent.message.completed`：收尾正文（回复通道）。
    for pending in events {
        if pending.event_type.as_str() != "agent.message.completed" {
            continue;
        }
        let view = message_view(pending)?;
        let Some(message_id) = view_text(&view, "messageId") else {
            continue;
        };
        let Ok(message_id) = MessageId::new(&message_id) else {
            continue;
        };
        let blocks = view
            .get("content")
            .and_then(|content| content.as_array())
            .cloned()
            .ok_or(PortError::InvalidRequest(
                "completed message content is not an array",
            ))?;
        let (state, reason) = match &terminal {
            Some((turn, state, reason)) if turn.as_ref() == pending.turn.as_ref() => {
                (*state, *reason)
            }
            // 没有同批终态（例如恢复补写）：收尾事件本身即「完整折叠」，按完成处理。
            _ => (MessageState::Completed, None),
        };
        let content = serde_json::to_string(&blocks)
            .map_err(|_| PortError::InvalidRequest("message content is not serializable"))?;
        sqlx::query(
            "INSERT INTO owned_message (message_id, session_id, turn_id, channel, state, \
             content_json, partial_reason, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'reply', ?4, ?5, ?6, ?7, ?7) \
             ON CONFLICT(message_id) DO UPDATE SET state = excluded.state, \
             content_json = excluded.content_json, partial_reason = excluded.partial_reason, \
             updated_at = excluded.updated_at, \
             turn_id = COALESCE(owned_message.turn_id, excluded.turn_id)",
        )
        .bind(message_id.as_str())
        .bind(session.as_str())
        .bind(pending.turn.as_ref().map(TurnId::as_str))
        .bind(state.as_str())
        .bind(&content)
        .bind(reason.map(PartialReason::as_str))
        .bind(at.as_str())
        .execute(&mut **tx)
        .await
        .db()?;
        // 媒体引用：只存引用，不存字节；不可用的内容（外部 URI）不产生引用。
        sqlx::query("DELETE FROM owned_media_ref WHERE message_id = ?1")
            .bind(message_id.as_str())
            .execute(&mut **tx)
            .await
            .db()?;
        for (position, block) in blocks.iter().enumerate() {
            let Some(block_type) = block.get("type").and_then(|value| value.as_str()) else {
                continue;
            };
            let Some(kind) = media_kind_of(block_type) else {
                continue;
            };
            let Some(attachment) = block.get("attachmentId").and_then(|value| value.as_str())
            else {
                continue;
            };
            let attachment_id = AttachmentId::new(attachment)
                .map_err(|_| PortError::InvalidRequest("media attachment id is malformed"))?;
            let stored: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM owned_attachment WHERE attachment_id = ?1")
                    .bind(attachment_id.as_str())
                    .fetch_optional(&mut **tx)
                    .await
                    .db()?;
            if stored.is_none() {
                return Err(PortError::InvalidRequest(
                    "media reference points to an attachment that is not sealed",
                ));
            }
            sqlx::query(
                "INSERT INTO owned_media_ref (message_id, position, attachment_id, kind) \
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(message_id.as_str())
            .bind(i64::try_from(position).map_err(|_| {
                PortError::InvalidRequest("media position exceeds the supported range")
            })?)
            .bind(attachment_id.as_str())
            .bind(kind.as_str())
            .execute(&mut **tx)
            .await
            .db()?;
        }
    }

    // 3. turn 终态：思考消息按终态收尾；回复消息只在停止/失败时标记部分结果（正常完成的回复由
    //    `agent.message.completed` 收尾，缺了它不得伪造成功）。
    if let Some((turn, state, reason)) = terminal {
        sqlx::query(
            "UPDATE owned_message SET state = ?1, partial_reason = ?2, updated_at = ?3 \
             WHERE session_id = ?4 AND channel = 'thought' AND state = 'partial' \
             AND (turn_id IS ?5)",
        )
        .bind(state.as_str())
        .bind(reason.map(PartialReason::as_str))
        .bind(at.as_str())
        .bind(session.as_str())
        .bind(turn.as_ref().map(TurnId::as_str))
        .execute(&mut **tx)
        .await
        .db()?;
        if state != MessageState::Completed {
            sqlx::query(
                "UPDATE owned_message SET state = ?1, partial_reason = ?2, updated_at = ?3 \
                 WHERE session_id = ?4 AND channel = 'reply' AND state = 'partial' \
                 AND (turn_id IS ?5)",
            )
            .bind(state.as_str())
            .bind(reason.map(PartialReason::as_str))
            .bind(at.as_str())
            .bind(session.as_str())
            .bind(turn.as_ref().map(TurnId::as_str))
            .execute(&mut **tx)
            .await
            .db()?;
        } else {
            // 正常终态但回复缺 `agent.message.completed`（适配器错误或崩溃窗口）：已收到的正文保留，
            // 但**不得**伪装成完整成功（§6 第 14 条）。`failed` + `agent_failed` 是唯一的诚实表达；
            // 留成 `partial` + `NULL` 原因会让 `PartialResult::try_new` 在读取时判为损坏。
            sqlx::query(
                "UPDATE owned_message SET state = 'failed', partial_reason = ?1, updated_at = ?2 \
                 WHERE session_id = ?3 AND channel = 'reply' AND state = 'partial' \
                 AND (turn_id IS ?4)",
            )
            .bind(PartialReason::AgentFailed.as_str())
            .bind(at.as_str())
            .bind(session.as_str())
            .bind(turn.as_ref().map(TurnId::as_str))
            .execute(&mut **tx)
            .await
            .db()?;
        }
    }
    Ok(())
}

/// prompt 受理时的清单绑定（§6 第 22 条）：在**受理事务内**复核并写入会话绑定。
///
/// 复核项：清单存在且已封存、属于该会话与**认证主体**、条目与清单登记逐项相同、每个附件仍存在且
/// 文件可读。任一失败 → 整事务回滚（命令不被受理、Agent 不会被派发）。
///
/// 成功绑定后**删除清单行**（条目随外键级联）：清单一旦被 prompt 消费就没有继续存在的理由，留着它
/// 会让 `owned_attachment_submission_item` 永久成为附件 GC 根，使存储随每次带附件的 prompt 无界增长。
async fn bind_submission(
    tx: &mut Transaction<'_, Sqlite>,
    session: &SessionId,
    binding: &SubmissionBinding,
    config: &StorageConfig,
    at: &Timestamp,
) -> Result<(), PortError> {
    let row = sqlx::query(
        "SELECT session_id, subject_kind, subject_id, state FROM owned_attachment_submission \
         WHERE submission_id = ?1",
    )
    .bind(binding.submission.as_str())
    .fetch_optional(&mut **tx)
    .await
    .db()?
    .ok_or_else(|| PortError::NotFound(EntityRef::Submission(binding.submission.clone())))?;
    if text(&row, "session_id")? != session.as_str() {
        return Err(PortError::InvalidRequest(
            "attachment manifest belongs to another session",
        ));
    }
    // 主体在事务内复核：调用方（用例层）的事前检查不能替代这条不变量，任何直接调用
    // `commit_with_submission` 的路径都必须经过这里。
    if text(&row, "subject_kind")? != binding.subject.kind()
        || text(&row, "subject_id")? != binding.subject.id_text()
    {
        return Err(PortError::InvalidRequest(
            "attachment manifest belongs to another subject",
        ));
    }
    if text(&row, "state")? != "sealed" {
        return Err(PortError::InvalidRequest(
            "attachment manifest is not sealed",
        ));
    }
    let registered: Vec<String> = sqlx::query_scalar(
        "SELECT attachment_id FROM owned_attachment_submission_item \
         WHERE submission_id = ?1 ORDER BY position ASC",
    )
    .bind(binding.submission.as_str())
    .fetch_all(&mut **tx)
    .await
    .db()?;
    if registered.len() != binding.attachments.len()
        || registered
            .iter()
            .zip(&binding.attachments)
            .any(|(stored, expected)| stored != expected.as_str())
    {
        return Err(PortError::InvalidRequest(
            "attachment binding does not match the sealed manifest",
        ));
    }
    for attachment in &binding.attachments {
        let relative: Option<String> = sqlx::query_scalar(
            "SELECT relative_path FROM owned_attachment WHERE attachment_id = ?1",
        )
        .bind(attachment.as_str())
        .fetch_optional(&mut **tx)
        .await
        .db()?;
        let Some(relative) = relative else {
            return Err(PortError::InvalidRequest(
                "attachment binding references an attachment that is not sealed",
            ));
        };
        let path = match relative.strip_prefix(&format!("{}/", migrate::ATTACHMENTS_DIR)) {
            Some(name) => config.attachment_dir.join(name),
            None => config.data_dir.join(&relative),
        };
        if !path.is_file() {
            return Err(PortError::Corrupt("attachment bytes are missing"));
        }
        let affected = sqlx::query(
            "INSERT INTO owned_attachment_link (session_id, attachment_id, generation, sha256) \
             SELECT ?1, a.attachment_id, 1, a.sha256 FROM owned_attachment a \
             WHERE a.attachment_id = ?2 \
             ON CONFLICT(session_id, attachment_id) DO UPDATE SET sha256 = excluded.sha256",
        )
        .bind(session.as_str())
        .bind(attachment.as_str())
        .execute(&mut **tx)
        .await
        .db()?
        .rows_affected();
        if affected != 1 {
            return Err(PortError::Corrupt(
                "attachment vanished while binding the submission",
            ));
        }
        // 去重后复用的旧内容保留首次上传的 `created_at`；绑定必须把「最近一次使用」推到本次
        // 受理时刻，否则下一轮 `prune` 会按首次上传时间删掉这条刚建立的链接（随后 `read_chunk`
        // 失去授权、24 小时后内容被 GC），而消息仍在正文保留期内。
        sqlx::query("UPDATE owned_attachment SET last_used_at = ?1 WHERE attachment_id = ?2")
            .bind(at.as_str())
            .bind(attachment.as_str())
            .execute(&mut **tx)
            .await
            .db()?;
    }
    // 清单的生命周期到此结束：条目随 `ON DELETE CASCADE` 一起删除，附件改由刚写入的
    // `owned_attachment_link`/后续消息的 `owned_media_ref` 保护。重复受理同一 requestId 的幂等命中
    // 发生在绑定之前（`accept_with_binding` 里 `replayed` 分支），因此重试不会因为清单已删除而失败。
    sqlx::query("DELETE FROM owned_attachment_submission WHERE submission_id = ?1")
        .bind(binding.submission.as_str())
        .execute(&mut **tx)
        .await
        .db()?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// SessionStore / ReadView
// ---------------------------------------------------------------------------------------------

#[async_trait]
impl SessionStore for SqliteStore {
    async fn commit(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        self.writable()?;
        self.commit_owned(commit, None, None).await
    }

    async fn commit_with_submission(
        &self,
        commit: OwnedCommit,
        binding: Option<SubmissionBinding>,
    ) -> Result<CommitOutcome, PortError> {
        self.writable()?;
        self.commit_owned(commit, None, binding).await
    }

    async fn commit_workspace_mkdir_terminal(
        &self,
        actor: &Actor,
        commit: OwnedCommit,
    ) -> Result<CommitOutcome, PortError> {
        self.writable()?;
        self.commit_owned(commit, Some(actor), None).await
    }

    /// 三态归约的事实读取（`session-activity` D1/D2）：一个只读事务里读交互与 turn 两侧，
    /// 口径与 §7.3 的落库语义一致。`pending` 与「已仲裁未确认」都算未解决，失效行一律不算。
    async fn session_activity(
        &self,
        session: &SessionId,
    ) -> Result<SessionActivityFacts, PortError> {
        let mut tx = self.pools.read.begin_with("BEGIN").await.db()?;
        let rows = sqlx::query(
            "SELECT interaction_id, state, turn_id FROM owned_interaction \
             WHERE session_id = ?1 AND invalidated_at IS NULL \
             AND (state = 'pending' OR (state = 'resolved' AND confirmed_at IS NULL)) \
             ORDER BY created_at ASC, interaction_id ASC",
        )
        .bind(session.as_str())
        .fetch_all(&mut *tx)
        .await
        .db()?;
        let mut unresolved = 0_u32;
        let mut unconfirmed = Vec::new();
        let mut by_turn: Vec<(TurnId, u32)> = Vec::new();
        for row in &rows {
            unresolved = unresolved.saturating_add(1);
            if text(row, "state")? == "resolved" {
                unconfirmed.push(decode::<InteractionId>(
                    &text(row, "interaction_id")?,
                    "owned_interaction.interaction_id",
                )?);
            }
            if let Some(turn) = opt_text(row, "turn_id")? {
                let turn: TurnId = decode(&turn, "owned_interaction.turn_id")?;
                match by_turn.iter().position(|(candidate, _)| candidate == &turn) {
                    Some(index) => {
                        by_turn[index].1 = by_turn[index].1.saturating_add(1);
                    }
                    None => by_turn.push((turn, 1)),
                }
            }
        }
        let unfinished: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM owned_turn WHERE session_id = ?1 \
             AND state IN ('queued','running','waiting_input','waiting_permission')",
        )
        .bind(session.as_str())
        .fetch_one(&mut *tx)
        .await
        .db()?;
        Ok(SessionActivityFacts {
            unresolved_interactions: unresolved,
            unconfirmed_interactions: unconfirmed,
            unresolved_by_turn: by_turn,
            unfinished_turns: u32::try_from(unfinished).unwrap_or(u32::MAX),
        })
    }

    /// 持久队列：`owned_turn.state = 'queued'`，按存储层分配的 `queue_index` 升序（D3）。
    async fn queued_turns(&self, session: &SessionId) -> Result<Vec<Turn>, PortError> {
        let rows = sqlx::query(
            "SELECT turn_id, session_id, state, queue_index, causation, started_at, ended_at \
             FROM owned_turn WHERE session_id = ?1 AND state = 'queued' \
             ORDER BY queue_index ASC",
        )
        .bind(session.as_str())
        .fetch_all(&self.pools.read)
        .await
        .db()?;
        let mut turns = Vec::with_capacity(rows.len());
        for row in &rows {
            turns.push(turn_from_row(row)?);
        }
        Ok(turns)
    }

    /// 启动恢复入口：仍有未结束 turn 或有效未解决交互的会话（上限由调用方给出）。
    async fn sessions_with_pending_activity(
        &self,
        limit: ReplayLimit,
    ) -> Result<Vec<SessionId>, PortError> {
        let rows = sqlx::query(
            "SELECT s.session_id FROM owned_session s \
             WHERE EXISTS (SELECT 1 FROM owned_turn t WHERE t.session_id = s.session_id \
                           AND t.state IN ('queued','running','waiting_input','waiting_permission')) \
                OR EXISTS (SELECT 1 FROM owned_interaction i WHERE i.session_id = s.session_id \
                           AND i.invalidated_at IS NULL \
                           AND (i.state = 'pending' \
                                OR (i.state = 'resolved' AND i.confirmed_at IS NULL))) \
             ORDER BY s.session_id ASC LIMIT ?1",
        )
        .bind(i64::from(limit.events()))
        .fetch_all(&self.pools.read)
        .await
        .db()?;
        let mut sessions = Vec::with_capacity(rows.len());
        for row in &rows {
            sessions.push(decode(
                &text(row, "session_id")?,
                "owned_session.session_id",
            )?);
        }
        Ok(sessions)
    }

    async fn load(&self, session: &SessionId) -> Result<Option<SessionSnapshot>, PortError> {
        let mut tx = self.pools.read.begin_with("BEGIN").await.db()?;
        let row = sqlx::query(&format!(
            "SELECT {SESSION_COLUMNS} FROM owned_session WHERE session_id = ?1"
        ))
        .bind(session.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let snapshot = SessionSnapshot {
            session: session_from_row(&row)?,
            origin_epoch: decode(&text(&row, "origin_epoch")?, "owned_session.origin_epoch")?,
            head: head_of(&mut tx).await?,
        };
        Ok(Some(snapshot))
    }

    async fn list(&self, query: SessionQuery) -> Result<Vec<SessionSummary>, PortError> {
        if query.only.as_ref().is_some_and(|only| only.is_empty()) {
            return Ok(Vec::new());
        }
        if query.limit == Some(0) {
            return Ok(Vec::new());
        }
        let mut sql = format!("SELECT {SESSION_SUMMARY_COLUMNS} FROM {SESSION_SUMMARY_FROM}");
        let mut clauses = Vec::new();
        let mut binds: Vec<String> = Vec::new();
        if let Some(only) = &query.only {
            clauses.push(format!("s.session_id IN ({})", placeholders(only.len())));
            binds.extend(only.iter().map(|id| id.as_str().to_owned()));
        }
        if !query.states.is_empty() {
            clauses.push(format!("s.state IN ({})", placeholders(query.states.len())));
            binds.extend(query.states.iter().map(|state| state.as_str().to_owned()));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY s.updated_at DESC, s.session_id ASC");
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        let mut statement = sqlx::query(&sql);
        for value in &binds {
            statement = statement.bind(value.as_str());
        }
        let rows = statement.fetch_all(&self.pools.read).await.db()?;
        let mut summaries = Vec::with_capacity(rows.len());
        for row in &rows {
            summaries.push(session_summary_from_row(row)?);
        }
        Ok(summaries)
    }

    async fn origin_epochs(
        &self,
        sessions: &[SessionId],
    ) -> Result<Vec<(SessionId, OriginEpoch)>, PortError> {
        if sessions.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT session_id, origin_epoch FROM owned_session WHERE session_id IN ({})",
            placeholders(sessions.len())
        );
        let mut statement = sqlx::query(&sql);
        for session in sessions {
            statement = statement.bind(session.as_str());
        }
        let rows = statement.fetch_all(&self.pools.read).await.db()?;
        let mut epochs = Vec::with_capacity(rows.len());
        for row in &rows {
            epochs.push((
                decode(&text(row, "session_id")?, "owned_session.session_id")?,
                decode(&text(row, "origin_epoch")?, "owned_session.origin_epoch")?,
            ));
        }
        Ok(epochs)
    }

    async fn head(&self) -> Result<GlobalCursor, PortError> {
        let mut tx = self.pools.read.begin_with("BEGIN").await.db()?;
        let head = head_of(&mut tx).await?;
        Ok(head)
    }

    async fn read_view(&self) -> Result<Box<dyn ReadView>, PortError> {
        let tx = self.pools.read.begin_with("BEGIN").await.db()?;
        Ok(Box::new(SqlReadView {
            server_epoch: self.server_epoch()?,
            tx: Mutex::new(tx),
        }))
    }

    async fn find_request(
        &self,
        request: &RequestId,
        actor: &Actor,
    ) -> Result<Option<CommandRecord>, PortError> {
        let (kind, actor_id) = actor_key(actor);
        let row = sqlx::query(&format!(
            "SELECT {COMMAND_COLUMNS} FROM owned_command \
             WHERE actor_kind = ?1 AND actor_id = ?2 AND request_id = ?3"
        ))
        .bind(kind.as_str())
        .bind(actor_id)
        .bind(request.as_str())
        .fetch_optional(&self.pools.read)
        .await
        .db()?;
        match row {
            Some(row) => Ok(Some(command_record_from_row(&row)?)),
            None => Ok(None),
        }
    }

    /// §3.6/§5.2：恢复所需的两个持久化取值的**窄读取**。
    ///
    /// 两列不进 `Session`/`SessionSummary`（`workspace_cwd` 是本机规范化路径，进入可投影形状会违反
    /// §3.6 的边界）；因此恢复路径只能经本入口取值。会话行不存在、或两列中任一为 `NULL` 时返回
    /// `Ok(None)`——`NULL` 是「该会话没有可用于恢复的数据」，**不是**错误，也**不得**被补全、推导或以
    /// 别名解析结果替换。取值形状非法（库被外部改写）按列值损坏报错，不返回半条记录。
    async fn load_recovery(
        &self,
        session: &SessionId,
    ) -> Result<Option<SessionRecoveryRecord>, PortError> {
        let row = sqlx::query(
            "SELECT agent_id, agent_name, agent_session_id, workspace_cwd FROM owned_session \
             WHERE session_id = ?1",
        )
        .bind(session.as_str())
        .fetch_optional(&self.pools.read)
        .await
        .db()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let agent = agent_from_row(&row)?;
        let agent_session_id = decode_opt(
            opt_text(&row, "agent_session_id")?,
            "owned_session.agent_session_id",
        )?;
        let workspace_cwd = opt_text(&row, "workspace_cwd")?;
        if agent_session_id.is_none() || workspace_cwd.is_none() {
            return Ok(None);
        }
        Ok(Some(SessionRecoveryRecord {
            agent,
            agent_session_id,
            workspace_cwd,
        }))
    }

    /// §5.2：该会话**仍可重放**的 `session_sequence` 下界/上界；没有可重放行时返回 `None`。
    /// §6 第 16 条：启动恢复要重新确认的命令行——`status = 'accepted'`、尚未拿到终态事件的
    /// **mutation** 行，按 `accepted_at` 升序（走 §7.3 的 `owned_command_status` 索引）。
    async fn unsettled_commands(
        &self,
        limit: ReplayLimit,
    ) -> Result<Vec<CommandRecord>, PortError> {
        let rows = sqlx::query(&format!(
            "SELECT {COMMAND_COLUMNS} FROM owned_command \
             WHERE status = 'accepted' AND terminal_event_id IS NULL AND kind = 'mutation' \
             ORDER BY accepted_at ASC, request_id ASC LIMIT ?1"
        ))
        .bind(i64::from(limit.events()))
        .fetch_all(&self.pools.read)
        .await
        .db()?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            records.push(command_record_from_row(row)?);
        }
        Ok(records)
    }

    async fn retention_window(
        &self,
        session: &SessionId,
    ) -> Result<Option<(Sequence, Sequence)>, PortError> {
        let row = sqlx::query(
            "SELECT MIN(session_sequence) AS low, MAX(session_sequence) AS high FROM owned_event \
             WHERE session_id = ?1 AND session_sequence IS NOT NULL",
        )
        .bind(session.as_str())
        .fetch_one(&self.pools.read)
        .await
        .db()?;
        match (opt_int(&row, "low")?, opt_int(&row, "high")?) {
            (Some(low), Some(high)) => Ok(Some((
                sequence(low, "retention window low")?,
                sequence(high, "retention window high")?,
            ))),
            _ => Ok(None),
        }
    }

    async fn prune(
        &self,
        policy: RetentionPolicy,
        at: Timestamp,
    ) -> Result<PruneReport, PortError> {
        self.writable()?;
        let mut tx = self.pools.write.begin_with("BEGIN IMMEDIATE").await.db()?;
        let mut sweep = Sweep::default();

        // ②′ 终态交互行：先解开将被清理事件的外键引用（§7.5 的顺序 ②′ → ① → ②）。
        sweep.interactions += sqlx::query(&format!(
            "DELETE FROM owned_interaction WHERE {TERMINAL_INTERACTION_PREDICATE}"
        ))
        .bind(at.as_str())
        .execute(&mut *tx)
        .await
        .db()?
        .rows_affected();
        // ① 过期 delta
        sweep.add(
            sweep_events(
                &mut tx,
                "kind = 'delta' AND expires_at IS NOT NULL AND expires_at <= ?1 \
                 AND NOT EXISTS (SELECT 1 FROM owned_interaction i \
                 WHERE i.request_event = owned_event.global_sequence)",
                &at,
            )
            .await?,
        );
        // ② 过期正文/结构化/状态事件
        sweep.add(
            sweep_events(
                &mut tx,
                "kind <> 'delta' AND expires_at IS NOT NULL AND expires_at <= ?1 \
                 AND NOT EXISTS (SELECT 1 FROM owned_interaction i \
                 WHERE i.request_event = owned_event.global_sequence)",
                &at,
            )
            .await?,
        );
        // ② 的伴生：终态交互行随正文窗口清理（`pending` 行保留，等待应答）。
        let interaction_threshold = threshold(&at, policy.transcript_retention_days)?;
        sweep.interactions += sqlx::query(
            "DELETE FROM owned_interaction WHERE state <> 'pending' AND created_at <= ?1",
        )
        .bind(interaction_threshold.as_str())
        .execute(&mut *tx)
        .await
        .db()?
        .rows_affected();
        // ③ 容量：只在超限时触发
        sweep.add(sweep_compacted(&mut tx, &policy, None, &self.measure_sql).await?);
        // 消息正文随正文保留期清理（媒体引用经外键级联删除；共享引用的附件仍由后续 GC 保留）。
        let message_threshold = threshold(&at, policy.transcript_retention_days)?;
        sweep.messages += sqlx::query("DELETE FROM owned_message WHERE updated_at <= ?1")
            .bind(message_threshold.as_str())
            .execute(&mut *tx)
            .await
            .db()?
            .rows_affected();
        // 消息清理时关联清理：附件的会话绑定（`owned_attachment_link`）只在附件仍处于消息保留窗口内、
        // 或仍被某条保留消息的 `owned_media_ref` 引用时继续存在。链接消失后，内容由 ④′ 的按引用 GC
        // 在闲置窗口后回收——这就是「消息保留期到期后附件被回收」的落点。
        //
        // 判定用「最近一次使用」而不是 `created_at`：内容寻址去重会让旧字节复用同一个附件行并在
        // 行上保留**首次**上传的 `created_at`，按它判定会把「今天重发 90 天前的同一文件」刚建立的
        // 链接在下一轮清理里删掉。`bind_submission`/`AttachmentStore::link` 在建立链接时把
        // `last_used_at` 刷新到绑定时刻，因此保留期从最近一次绑定起算；未再被使用的旧附件照常回收。
        sqlx::query(
            "DELETE FROM owned_attachment_link \
             WHERE NOT EXISTS (SELECT 1 FROM owned_media_ref r \
                               WHERE r.attachment_id = owned_attachment_link.attachment_id) \
               AND NOT EXISTS (SELECT 1 FROM owned_attachment a \
                               WHERE a.attachment_id = owned_attachment_link.attachment_id \
                                 AND a.last_used_at > ?1)",
        )
        .bind(message_threshold.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        // ④″ 传输与清单的生命周期：未绑定/未封存的清单与闲置的 receiving 传输在这里过期（与
        // `prune_transfers` 同一实现）。必须先于附件的按引用 GC：否则已过期清单的条目会在**本轮**
        // 继续把附件挡在 GC 根之外。
        let (_, _, temporary) =
            crate::transfers::expire_stale_uploads(&mut tx, &self.config, &at, 1000).await?;
        // ④ 附件 LRU（预算来自 §7.5 的 `storage.attachment_max_total_bytes`）
        let (removed, freed, mut files) =
            sweep_attachments(&mut tx, self.config.attachment_max_total_bytes, &at).await?;
        sweep.attachments += removed;
        sweep.freed_bytes += freed;
        // ④′ 按引用的附件 GC：无会话绑定、无媒体引用、不在活跃清单里且闲置超过 24 小时的内容可回收。
        let gc_cutoff_text = window::shift_days(at.as_str(), -1).ok_or(
            StorageError::InvalidRequest("timestamp is outside the supported window"),
        )?;
        let gc_cutoff = Timestamp::new(&gc_cutoff_text)
            .map_err(|_| StorageError::InvalidRequest("timestamp shape"))?;
        let (removed, freed, unreferenced) =
            sweep_unreferenced_attachments(&mut tx, &gc_cutoff, &at).await?;
        sweep.attachments += removed;
        sweep.freed_bytes += freed;
        files.extend(unreferenced);
        // ⑤ 到期审计
        let audit_threshold = threshold(&at, policy.audit_retention_days)?;
        sweep.audit = sqlx::query("DELETE FROM owned_audit WHERE at <= ?1")
            .bind(audit_threshold.as_str())
            .execute(&mut *tx)
            .await
            .db()?
            .rows_affected();
        // §7.5：`last_prune_at` 随每次 prune 前进。
        sqlx::query("UPDATE meta SET value = ?1 WHERE key = 'last_prune_at'")
            .bind(at.as_str())
            .execute(&mut *tx)
            .await
            .db()?;
        let still_over_limit = over_capacity(&mut tx, &policy, None, &self.measure_sql).await?;
        tx.commit().await.db()?;
        remove_attachment_files(&self.config, &files);
        // 传输临时文件（不在 `owned_attachment` 里）在提交之后删除：崩溃最坏留下一个孤儿文件，而
        // 孤儿回收不会碰 `.transfers/` 子目录，因此这里必须自己删。
        for path in temporary {
            let _ = std::fs::remove_file(path);
        }

        Ok(PruneReport {
            removed_events: sweep.events,
            removed_interactions: sweep.interactions,
            removed_attachments: sweep.attachments,
            removed_audit: sweep.audit,
            removed_messages: sweep.messages,
            freed_bytes: sweep.freed_bytes,
            still_over_limit,
        })
    }

    async fn health(&self) -> Result<StoreHealth, PortError> {
        let user_version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&self.pools.read)
            .await
            .db()?;
        let mut read = self.pools.read.acquire().await.db()?;
        let total_bytes = measure_total(&mut read, &self.measure_sql).await?;
        drop(read);
        Ok(StoreHealth {
            total_bytes,
            integrity_ok: self.integrity_ok,
            read_only: !self.integrity_ok,
            user_version: u32::try_from(user_version).unwrap_or(0),
            owned_schema_version: u32::try_from(self.meta.owned_schema_version).unwrap_or(0),
            server_epoch: self.server_epoch()?,
        })
    }
}

/// §3.2：head = `{serverEpoch, globalSequence}`，序号是该视图内 `global_sequence` 的最大值。
async fn head_of(tx: &mut Transaction<'_, sqlx::Sqlite>) -> Result<GlobalCursor, StorageError> {
    let rows = sqlx::query("SELECT value FROM meta WHERE key = 'server_epoch'")
        .fetch_one(&mut **tx)
        .await?;
    let epoch: ServerEpoch = decode(&text(&rows, "value")?, "meta.server_epoch")?;
    let max: Option<i64> = sqlx::query_scalar("SELECT MAX(global_sequence) FROM owned_event")
        .fetch_one(&mut **tx)
        .await
        .db()?;
    let sequence = match max {
        Some(value) => sequence(value, "owned_event.global_sequence")?,
        None => Sequence::new(0).map_err(|_| StorageError::Corrupt("sequence zero"))?,
    };
    Ok(GlobalCursor::new(epoch, sequence))
}

/// 只读一致性视图：持有一个显式只读事务，三个查询都在同一快照上完成（§6 第 12 条）。
pub struct SqlReadView {
    server_epoch: ServerEpoch,
    tx: Mutex<Transaction<'static, sqlx::Sqlite>>,
}

impl std::fmt::Debug for SqlReadView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlReadView").finish_non_exhaustive()
    }
}

impl SqlReadView {
    /// 游标校验（§9.2 的四种拒绝原因里属于存储层的三种）。
    ///
    /// `beyond_head` 在合同里是**协议层**的 `sync.cursor_invalid` 原因（`SYNC_PROTOCOL.md` §9.2），
    /// 而 `ResetReason` 只有 `initial_sync|epoch_mismatch|cursor_expired|cache_incompatible` 四种取值，
    /// 因此「超过 head」返回 `InvalidRequest` 让 broker 映射成 `cursor_invalid/beyond_head`，不由存储层
    /// 假装成 reset。
    async fn resolve_cursor(
        &self,
        tx: &mut Transaction<'static, sqlx::Sqlite>,
        after: Option<&GlobalCursor>,
    ) -> Result<Option<ResetReason>, PortError> {
        let head = head_of(tx).await?;
        let Some(after) = after else {
            return Ok(Some(ResetReason::InitialSync));
        };
        if after.server_epoch != self.server_epoch {
            return Ok(Some(ResetReason::EpochMismatch));
        }
        if after.global_sequence > head.global_sequence {
            return Err(PortError::InvalidRequest("cursor is beyond head"));
        }
        let low: Option<i64> = sqlx::query_scalar("SELECT MIN(global_sequence) FROM owned_event")
            .fetch_one(&mut **tx)
            .await
            .db()?;
        match low {
            Some(low) if after.global_sequence.get() + 1 < u64::try_from(low).unwrap_or(0) => {
                Ok(Some(ResetReason::CursorExpired))
            }
            _ => Ok(None),
        }
    }
}

/// 历史页条目类型谓词：`event_type IN ('turn.queued',…)`。
///
/// 集合取自 [`acp_core::broker::HISTORY_ENTRY_EVENT_TYPES`]（`docs/SYNC_PROTOCOL.md` §10.2.1 的 17 类）
/// ——core 与存储**共用同一个常量**，本文件不另写一份字面量，替身 `FakeReadView` 也因此与真实存储同口径。
/// 取值都是 core 侧常量（只有 `[a-z]` 与 `.`），直接拼进 SQL 文本即可，不需要绑定参数。
///
/// 真正的约束是**谓词必须与 `limit`/页标记在同一条语句里生效**：事后过滤会让 `limit + 1` 的多取与
/// `history_has_earlier` 按未过滤的行计数，客户端会拿到空页或短页却被页标记告知「更早还有条目」，
/// 而它按「上一页最早条目的会话内序号」续读时又没有游标可用。
fn history_entry_predicate() -> String {
    let mut predicate = String::from("event_type IN (");
    for (index, event_type) in HISTORY_ENTRY_EVENT_TYPES.iter().enumerate() {
        if index > 0 {
            predicate.push(',');
        }
        predicate.push('\'');
        predicate.push_str(event_type);
        predicate.push('\'');
    }
    predicate.push(')');
    predicate
}

async fn read_session_locked(
    view: &SqlReadView,
    guard: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    query: HistoryQuery,
) -> Result<HistoryPage, PortError> {
    let head = head_of(&mut *guard).await?;
    let row = sqlx::query(&format!(
        "SELECT {SESSION_SUMMARY_COLUMNS} FROM {SESSION_SUMMARY_FROM} WHERE s.session_id = ?1"
    ))
    .bind(query.session.as_str())
    .fetch_optional(&mut **guard)
    .await
    .db()?;
    let summary = match row {
        Some(row) => session_summary_from_row(&row)?,
        None => {
            return Err(PortError::NotFound(EntityRef::Session(
                query.session.clone(),
            )));
        }
    };
    let from = query
        .after
        .as_ref()
        .map_or(0, |cursor| cursor.global_sequence.get());
    let from = storable(from)?;
    let fetch = i64::from(query.limit.events()) + 1;
    // `messages` 是 `messages` 面的原料开关（§11.5 的 `include`）：开启时给出该会话 `after` 之后的
    // 一页事件**连同持久化正文**——正文与定位来自同一条行（`EVENT_COLUMNS`），不逐条回查。
    let events = if query.include.messages {
        let rows = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM owned_event WHERE session_id = ?1 \
             AND global_sequence > ?2 AND compacted_into IS NULL \
             ORDER BY global_sequence ASC LIMIT ?3"
        ))
        .bind(query.session.as_str())
        .bind(from)
        .bind(fetch)
        .fetch_all(&mut **guard)
        .await
        .db()?;
        let mut events = Vec::with_capacity(rows.len().min(query.limit.events() as usize));
        for (index, row) in rows.iter().enumerate() {
            if index as u32 >= query.limit.events() {
                break;
            }
            events.push(owned_event_from_row(row)?);
        }
        events
    } else {
        Vec::new()
    };
    // 结构化历史条目（§11.5 的 `history` 取值，`design.md` D2/D4）：**一条查询**按会话内序号取行，
    // 同一行里一并取出视图与 ACP 两列，因此不存在逐条取正文的 N+1。
    //
    // 条目集合不是「该会话所有可读事件」：`SYNC_PROTOCOL.md` §10.2.1 的 17 类之外的事件（承载消息
    // 正文的增量与收尾、摘要/仪表类、命令终态类、连接与审计类）都不产生条目。谓词因此写在**本条
    // 语句内**——它与 `limit + 1` 的多取、页标记出自同一次计数，事后过滤会让客户端拿到空页或短页
    // 却被告知「更早还有条目」，而它按「上一页最早条目的会话内序号」续读时没有游标可用。
    //
    // 分页走已有的唯一索引 `owned_event_session(session_id, session_sequence)`：降序取 `limit + 1`
    // 条再翻回升序，于是「省略游标 = 最新一页」与「带游标 = 紧接该游标之前的一页」共用一条语句，
    // 多取到的那一条即页标记。`session_sequence IS NOT NULL` 是**写入端不变量**的显式复述：会话级
    // 提交总为 `session_sequence` 绑定 `Some`（§9 判据 17），缺列的行（外部改写）不能被编一个序号
    // 当作条目——schema 层并未强制这一点（§7.3 的成对 CHECK 只约束非会话行，且 SQLite 的 UNIQUE
    // 视 NULL 互不相等）。
    let before = query.before_history.as_ref().map(|cursor| cursor.get());
    let bound = before.map(storable).transpose()?;
    let (history, history_has_earlier) = if query.include.history {
        let entries = history_entry_predicate();
        let rows = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM owned_event WHERE session_id = ?1 \
             AND compacted_into IS NULL AND session_sequence IS NOT NULL AND {entries} \
             AND (?2 IS NULL OR session_sequence < ?2) \
             ORDER BY session_sequence DESC LIMIT ?3"
        ))
        .bind(query.session.as_str())
        .bind(bound)
        .bind(fetch)
        .fetch_all(&mut **guard)
        .await
        .db()?;
        let has_earlier = rows.len() as u32 > query.limit.events();
        let mut history = Vec::with_capacity(rows.len().min(query.limit.events() as usize));
        for row in rows.iter().take(query.limit.events() as usize) {
            history.push(owned_event_from_row(row)?);
        }
        history.reverse();
        // 空页 + 游标：要么游标正好指向仍可读的最早一条（合法，页标记为假、不返回占位），要么它落在
        // 已按保留策略压缩/清理的区间——后者必须显式报保留期类错误，不得静默返回空页让调用方误以为
        // 已到最早一条（§11.5 的「保留窗口之前显式不可读」）。非空页已经证明游标高于该下界。下界也
        // 只按条目集合算（游标只可能来自上一页的条目）：非条目事件在游标之下不构成「仍可读的条目」。
        if let Some(before) = before
            && !has_earlier
            && history.is_empty()
        {
            let low: Option<i64> = sqlx::query_scalar(&format!(
                "SELECT MIN(session_sequence) FROM owned_event WHERE session_id = ?1 \
                 AND compacted_into IS NULL AND session_sequence IS NOT NULL AND {entries}",
            ))
            .bind(query.session.as_str())
            .fetch_one(&mut **guard)
            .await
            .db()?;
            let expired = match low {
                Some(value) => {
                    before
                        < u64::try_from(value).map_err(|_| {
                            StorageError::Corrupt("session_sequence window bound is negative")
                        })?
                }
                // 可读窗口内一条都没有：任何游标都指向已不可读的区间。
                None => true,
            };
            if expired {
                return Err(PortError::InvalidRequest(
                    "history cursor precedes the retained window",
                ));
            }
        }
        (history, has_earlier)
    } else {
        (Vec::new(), false)
    };
    // 消息正文页（`owned_message` 的权威正文）：按 `(created_at, message_id)` 复合游标取**一页**，
    // 而不是整段会话历史。`messages_limit = 0` 或未开 `include.messages` 时完全不查这两张表。
    let (messages, messages_has_earlier) = if query.include.messages
        && query.messages_limit.events() > 0
    {
        let page_size = query.messages_limit.events();
        let fetch = i64::from(page_size) + 1;
        let (before_created, before_id) = match &query.before_messages {
            Some(cursor) => (
                Some(cursor.created_at.as_str().to_owned()),
                Some(cursor.message_id.as_str().to_owned()),
            ),
            None => (None, None),
        };
        let rows = sqlx::query(
            "SELECT message_id, session_id, turn_id, channel, state, content_json, \
                        partial_reason, created_at, updated_at \
                 FROM owned_message WHERE session_id = ?1 \
                 AND (?2 IS NULL OR created_at < ?2 OR (created_at = ?2 AND message_id < ?3)) \
                 ORDER BY created_at DESC, message_id DESC LIMIT ?4",
        )
        .bind(query.session.as_str())
        .bind(before_created.as_deref())
        .bind(before_id.as_deref())
        .bind(fetch)
        .fetch_all(&mut **guard)
        .await
        .db()?;
        let has_earlier = rows.len() as u32 > page_size;
        let mut rows = rows;
        rows.truncate(page_size as usize);
        // 取的时候按倒序（最新一页），返回给调用方时翻回升序。
        rows.reverse();
        let ids: Vec<String> = rows
            .iter()
            .map(|row| text(row, "message_id"))
            .collect::<Result<_, _>>()?;
        // media 只查本页消息（`IN (…)` 的占位符数量 = 本页消息数，必然有界）。
        let media_rows = if ids.is_empty() {
            Vec::new()
        } else {
            let placeholders = vec!["?"; ids.len()].join(", ");
            let sql = format!(
                "SELECT r.message_id, r.position, r.attachment_id, r.kind, a.media_type, \
                            a.byte_length, a.sha256 \
                     FROM owned_media_ref r \
                     JOIN owned_attachment a ON a.attachment_id = r.attachment_id \
                     WHERE r.message_id IN ({placeholders}) \
                     ORDER BY r.message_id ASC, r.position ASC"
            );
            let mut media_query = sqlx::query(&sql);
            for id in &ids {
                media_query = media_query.bind(id);
            }
            media_query.fetch_all(&mut **guard).await.db()?
        };
        let mut by_message: std::collections::HashMap<String, Vec<MediaReference>> =
            std::collections::HashMap::new();
        for row in &media_rows {
            let message_id = text(row, "message_id")?;
            let kind: MediaKind =
                text(row, "kind")?
                    .parse()
                    .map_err(|_| StorageError::ColumnValue {
                        column: "owned_media_ref.kind",
                        expected: "image|audio|file",
                    })?;
            let reference = MediaReference::try_new(
                decode(
                    &text(row, "attachment_id")?,
                    "owned_media_ref.attachment_id",
                )?,
                query.session.clone(),
                kind,
                &text(row, "media_type")?,
                u64::try_from(int(row, "byte_length")?).unwrap_or(0),
                decode(&text(row, "sha256")?, "owned_attachment.sha256")?,
                None,
                acp_core::model::MediaAvailability::Available,
            )
            .map_err(|_| PortError::Corrupt("stored media reference is invalid"))?;
            by_message.entry(message_id).or_default().push(reference);
        }
        let mut messages = Vec::with_capacity(rows.len());
        for row in &rows {
            let message_id_text = text(row, "message_id")?;
            let channel: MessageChannel =
                text(row, "channel")?
                    .parse()
                    .map_err(|_| StorageError::ColumnValue {
                        column: "owned_message.channel",
                        expected: "reply|thought",
                    })?;
            let state: MessageState =
                text(row, "state")?
                    .parse()
                    .map_err(|_| StorageError::ColumnValue {
                        column: "owned_message.state",
                        expected: "partial|completed|failed|cancelled",
                    })?;
            let reason: Option<PartialReason> = match opt_text(row, "partial_reason")? {
                Some(text) => Some(text.parse().map_err(|_| StorageError::ColumnValue {
                    column: "owned_message.partial_reason",
                    expected: "agent_stopped|agent_failed|turn_cancelled|connection_lost|retention_expired",
                })?),
                None => None,
            };
            messages.push(
                MessageRecord::try_new(
                    decode(&message_id_text, "owned_message.message_id")?,
                    query.session.clone(),
                    decode_opt(opt_text(row, "turn_id")?, "owned_message.turn_id")?,
                    channel,
                    PartialResult::try_new(state, reason)
                        .map_err(|_| PortError::Corrupt("stored message result is invalid"))?,
                    acp_core::model::JsonValueText::new(&text(row, "content_json")?)
                        .map_err(|_| PortError::Corrupt("stored message content is not json"))?,
                    by_message.remove(&message_id_text).unwrap_or_default(),
                    decode(&text(row, "created_at")?, "owned_message.created_at")?,
                    decode(&text(row, "updated_at")?, "owned_message.updated_at")?,
                )
                .map_err(|_| PortError::Corrupt("stored message is invalid"))?,
            );
        }
        (messages, has_earlier)
    } else {
        (Vec::new(), false)
    };
    let turns = if query.include.turns {
        let rows = sqlx::query(&format!(
            "SELECT {TURN_COLUMNS} FROM owned_turn WHERE session_id = ?1 ORDER BY queue_index ASC"
        ))
        .bind(query.session.as_str())
        .fetch_all(&mut **guard)
        .await
        .db()?;
        let mut turns = Vec::with_capacity(rows.len());
        for row in &rows {
            turns.push(turn_from_row(row)?);
        }
        turns
    } else {
        Vec::new()
    };
    let interactions = if query.include.pending_interactions {
        let rows = sqlx::query(&format!(
            "SELECT {INTERACTION_COLUMNS} FROM owned_interaction WHERE session_id = ?1 \
             AND state = 'pending' AND invalidated_at IS NULL \
             ORDER BY created_at ASC, interaction_id ASC"
        ))
        .bind(query.session.as_str())
        .fetch_all(&mut **guard)
        .await
        .db()?;
        let mut interactions = Vec::with_capacity(rows.len());
        for row in &rows {
            // §7.3 的 `owned_interaction` 没有 options 列（列集合是冻结的）：候选项由
            // `request_event` 指向的交互事件 payload 承载，因此持久层只投影元数据。
            interactions.push(
                PendingInteraction::try_new(
                    decode(
                        &text(row, "interaction_id")?,
                        "owned_interaction.interaction_id",
                    )?,
                    decode(&text(row, "kind")?, "owned_interaction.kind")?,
                    decode(&text(row, "session_id")?, "owned_interaction.session_id")?,
                    decode(&text(row, "created_at")?, "owned_interaction.created_at")?,
                    Vec::<InteractionOption>::new(),
                )
                .map_err(|_| StorageError::ColumnValue {
                    column: "owned_interaction",
                    expected: "pending interaction metadata",
                })?,
            );
        }
        interactions
    } else {
        Vec::new()
    };
    let next = if events.len() as u32 == query.limit.events()
        && let Some(last) = events.last()
    {
        if last.event.global_sequence.get() < head.global_sequence.get() {
            Some(GlobalCursor::new(
                view.server_epoch.clone(),
                last.event.global_sequence,
            ))
        } else {
            None
        }
    } else {
        None
    };
    Ok(HistoryPage {
        session: summary,
        events,
        history,
        history_has_earlier,
        messages,
        messages_has_earlier,
        turns,
        interactions,
        // 活体数据由用例层合并（`HistoryPage` 的文档注释）：持久层不保存 config/capabilities。
        config: Vec::new(),
        capabilities: None,
        head,
        next,
    })
}

#[async_trait]
impl ReadView for SqlReadView {
    async fn head(&self) -> Result<GlobalCursor, PortError> {
        let mut guard = self.tx.lock().await;
        Ok(head_of(&mut guard).await?)
    }

    async fn replay(
        &self,
        after: Option<GlobalCursor>,
        limit: ReplayLimit,
    ) -> Result<ReplayBatch, PortError> {
        let mut guard = self.tx.lock().await;
        let head = head_of(&mut guard).await?;
        let reset_required = self.resolve_cursor(&mut guard, after.as_ref()).await?;
        let from = match reset_required {
            // 需要重建快照时从当前窗口的下界开始（窗口之前的正文已经不在库里）。
            Some(_) => 0_i64,
            None => storable(
                after
                    .as_ref()
                    .map_or(0, |cursor| cursor.global_sequence.get()),
            )?,
        };
        let rows = sqlx::query(&format!(
            // §6 第 15 条：被压缩的 delta 由 summary 事件替代，重放里不再出现。
            "SELECT {EVENT_COLUMNS} FROM owned_event WHERE global_sequence > ?1 \
             AND compacted_into IS NULL ORDER BY global_sequence ASC LIMIT ?2"
        ))
        .bind(from)
        .bind(i64::from(limit.events()) + 1)
        .fetch_all(&mut **guard)
        .await
        .db()?;
        let mut events = Vec::with_capacity(rows.len().min(limit.events() as usize));
        let mut next = None;
        for (index, row) in rows.iter().enumerate() {
            if index as u32 >= limit.events() {
                // 还有更多：续读游标停在本批最后一条。
                break;
            }
            events.push(event_from_row(row)?);
        }
        if rows.len() as u32 > limit.events()
            && let Some(event) = events.last()
        {
            next = Some(GlobalCursor::new(
                self.server_epoch.clone(),
                event.global_sequence,
            ));
        }
        Ok(ReplayBatch {
            events,
            head,
            next,
            reset_required,
        })
    }

    /// Sync 事件读面（`SYNC_PROTOCOL.md` §10.1 的 `event` 帧）：`after` 之后的一页事件 + 这批数据的
    /// head（`sync.caught_up` 的屏障游标），三个查询都在**同一次只读事务**（本视图持有的快照）内完成。
    ///
    /// 三个要点：
    ///
    /// - **窗口**：`global_sequence > after` 严格升序、`LIMIT limit`；已压缩的 delta 不出现（与
    ///   `replay` 同口径，§6 第 15 条：被压行由 summary 事件替代）。cursor 的 epoch/窗口解释权不在
    ///   本层（`replay` 的四种拒绝原因由投递层判定）。
    /// - **来源列**：`origin_kind` 与 `causation` 都取事件行自己的列（`EVENT_COLUMNS` 已选中）。
    ///   **五个**取值原样往返；词表外的 `origin_kind` 是列值损坏 → `Corrupt`（不降级、不猜测；DDL 的
    ///   CHECK 让它在正常写入路径上不可达，这里是读取侧的失败关闭）。**不**用「这条事件有没有
    ///   `session_id`」推断来源。
    /// - **`origin.deviceId`/`origin.nodeId`**：一条批量查询一次回查 device 与 node 两类 actor
    ///   （不是逐条），读条目时 device 事件只认设备行、node 事件只认节点行。得不出唯一取值时**整批
    ///   读失败关闭**（`Corrupt`）：§10.1 的配对规则要求这两类事件带非空身份字段，而升级前写入的历史
    ///   行在 `causation`/actor 分类上可能本来就不可归因——读面必须把这一点显式暴露给投递层，而不是
    ///   给一个 `None` 让它静默发出违反 §10.1 的帧。
    async fn sync_replay(
        &self,
        after: Option<GlobalCursor>,
        limit: ReplayLimit,
    ) -> Result<SyncReplaySlice, PortError> {
        let mut guard = self.tx.lock().await;
        let head = head_of(&mut guard).await?;
        let from = storable(
            after
                .as_ref()
                .map_or(0, |cursor| cursor.global_sequence.get()),
        )?;
        let rows = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM owned_event WHERE global_sequence > ?1 \
             AND compacted_into IS NULL ORDER BY global_sequence ASC LIMIT ?2"
        ))
        .bind(from)
        .bind(i64::from(limit.events()))
        .fetch_all(&mut **guard)
        .await
        .db()?;
        let origins = command_origins(&mut guard, &rows).await?;
        let mut events = Vec::with_capacity(rows.len());
        for row in &rows {
            events.push(sync_event_from_row(row, &origins)?);
        }
        Ok(SyncReplaySlice { events, head })
    }

    /// §9.16：按事件 id 还原正文。
    ///
    /// `view` 直接取库内 `payload_json` 的**原样文本**（只经 `ViewJson` 做形状校验，不重新序列化，因此
    /// 未知字段与键序都逐字节保留）；`acp` 按 `acp_raw_unavailable_reason` 分两路还原，最后按
    /// `AcpRaw::validate` 自检一次（字节数必须与原文一致）。
    async fn event_payload(&self, event: &EventId) -> Result<Option<EventPayload>, PortError> {
        let mut guard = self.tx.lock().await;
        let row = sqlx::query(
            "SELECT payload_json, acp_media_type, acp_raw_json, acp_byte_length, acp_sha256, \
             acp_raw_unavailable_reason FROM owned_event WHERE event_id = ?1",
        )
        .bind(event.as_str())
        .fetch_optional(&mut **guard)
        .await
        .db()?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(payload_from_row(&row)?))
    }

    /// Node Link 的会话读视图（`NODE_LINK_PROTOCOL.md` §12.4）：一次只读事务内取回「快照元数据
    /// （会话摘要、origin head、未决交互）+ `after` 之后的事件与正文」。
    ///
    /// head 取该会话已提交的最大 `origin_sequence`，epoch 取该会话行上的值：清理按 `global_sequence`
    /// 升序删旧行，因此最大值不会因保留期清理而丢失。会话不存在返回 `NotFound`。
    async fn node_link_slice(
        &self,
        session: &SessionId,
        after: Option<OriginCursor>,
        limit: ReplayLimit,
    ) -> Result<NodeLinkSlice, PortError> {
        let mut guard = self.tx.lock().await;
        let row = sqlx::query(&format!(
            "SELECT {SESSION_SUMMARY_COLUMNS} FROM {SESSION_SUMMARY_FROM} WHERE s.session_id = ?1"
        ))
        .bind(session.as_str())
        .fetch_optional(&mut **guard)
        .await
        .db()?;
        let Some(row) = row else {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        };
        let summary = session_summary_from_row(&row)?;
        let origin_epoch: OriginEpoch =
            decode(&text(&row, "origin_epoch")?, "owned_session.origin_epoch")?;
        let from = after
            .as_ref()
            .map_or(0, |cursor| cursor.origin_sequence.get());
        let rows = sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM owned_event WHERE session_id = ?1 \
             AND origin_sequence > ?2 AND compacted_into IS NULL \
             ORDER BY origin_sequence ASC LIMIT ?3"
        ))
        .bind(session.as_str())
        .bind(storable(from)?)
        .bind(i64::from(limit.events()))
        .fetch_all(&mut **guard)
        .await
        .db()?;
        let mut events = Vec::with_capacity(rows.len());
        for row in &rows {
            events.push(owned_event_from_row(row)?);
        }
        let head_sequence: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(origin_sequence) FROM owned_event WHERE session_id = ?1",
        )
        .bind(session.as_str())
        .fetch_one(&mut **guard)
        .await
        .db()?;
        let head_sequence = match head_sequence {
            Some(value) => sequence(value, "owned_event.origin_sequence")?,
            None => Sequence::new(0).map_err(|_| StorageError::Corrupt("sequence zero"))?,
        };
        // 未决交互的创建事件由 `request_event`（`owned_event.global_sequence`）定位，取它的
        // `event_id`：wire 的 `pending_interactions[].payloadDigest` 需要那个事件的 payload。
        // 创建事件受 §7.5 的清理谓词保护（引用它的事件不会被删）；关联缺失只能来自合同外的直接写入，
        // 此时**失败关闭**而不是静默少投影一个条目。
        let interactions = sqlx::query(
            "SELECT i.interaction_id, i.kind, i.session_id, i.created_at, e.event_id \
             FROM owned_interaction i \
             LEFT JOIN owned_event e ON e.global_sequence = i.request_event \
             WHERE i.session_id = ?1 AND i.state = 'pending' AND i.invalidated_at IS NULL \
             ORDER BY i.created_at ASC, i.interaction_id ASC",
        )
        .bind(session.as_str())
        .fetch_all(&mut **guard)
        .await
        .db()?;
        let mut pending_interactions = Vec::with_capacity(interactions.len());
        for row in &interactions {
            let interaction = PendingInteraction::try_new(
                decode(
                    &text(row, "interaction_id")?,
                    "owned_interaction.interaction_id",
                )?,
                decode(&text(row, "kind")?, "owned_interaction.kind")?,
                decode(&text(row, "session_id")?, "owned_interaction.session_id")?,
                decode(&text(row, "created_at")?, "owned_interaction.created_at")?,
                Vec::<InteractionOption>::new(),
            )
            .map_err(|_| StorageError::ColumnValue {
                column: "owned_interaction",
                expected: "pending interaction metadata",
            })?;
            let Some(event_id) = opt_text(row, "event_id")? else {
                return Err(PortError::Corrupt(
                    "pending interaction without a paired origin event",
                ));
            };
            let origin_event = decode(&event_id, "owned_event.event_id")?;
            pending_interactions.push(PendingInteractionOrigin {
                interaction,
                origin_event,
            });
        }
        Ok(NodeLinkSlice {
            summary,
            head: OriginCursor {
                origin_epoch,
                origin_sequence: head_sequence,
            },
            pending_interactions,
            events,
        })
    }

    async fn session_event_payload(
        &self,
        session: &SessionId,
        event: &EventId,
    ) -> Result<Option<EventPayload>, PortError> {
        let mut guard = self.tx.lock().await;
        let row = sqlx::query(
            "SELECT payload_json, acp_media_type, acp_raw_json, acp_byte_length, acp_sha256, \
             acp_raw_unavailable_reason FROM owned_event WHERE event_id = ?1 AND session_id = ?2",
        )
        .bind(event.as_str())
        .bind(session.as_str())
        .fetch_optional(&mut **guard)
        .await
        .db()?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(payload_from_row(&row)?))
    }

    async fn read_session(&self, query: HistoryQuery) -> Result<HistoryPage, PortError> {
        let mut guard = self.tx.lock().await;
        read_session_locked(self, &mut guard, query).await
    }

    async fn read_session_origin(
        &self,
        query: HistoryQuery,
    ) -> Result<acp_core::ports::OriginReadPage, PortError> {
        let mut guard = self.tx.lock().await;
        // R 必须在同一事务内捕获：页数据、来源位置与覆盖都来自这一个只读快照。
        let page = read_session_locked(self, &mut guard, query.clone()).await?;
        let (read_cursor, coverage) = session_coverage(&mut guard, &query, &page.messages).await?;
        Ok(acp_core::ports::OriginReadPage {
            page,
            read_cursor,
            coverage,
        })
    }
}

/// Owner 侧读取的来源位置与逐消息连续覆盖（与 `read_session_locked` 共用同一事务）。
///
/// R = 会话当前 head（`session_sequence`），epoch 取 `owned_session.origin_epoch`。每条返回消息的
/// `lastDeltaIndex` 是**从 0 开始连续存在**的最后一个 `agent.message.delta` 索引；没有 delta 行时为
/// `None`（页面没有拼入任何增量）。已压缩的 delta 不在可读窗口内，不能据此声称覆盖。
async fn session_coverage(
    guard: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    query: &HistoryQuery,
    messages: &[acp_core::model::MessageRecord],
) -> Result<
    (
        acp_core::model::OriginCursor,
        Vec<acp_core::ports::MessageCoverage>,
    ),
    PortError,
> {
    let epoch: String =
        sqlx::query_scalar("SELECT origin_epoch FROM owned_session WHERE session_id = ?1")
            .bind(query.session.as_str())
            .fetch_optional(&mut **guard)
            .await
            .db()?
            .ok_or_else(|| PortError::NotFound(EntityRef::Session(query.session.clone())))?;
    let head: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(session_sequence) FROM owned_event WHERE session_id = ?1 AND session_sequence IS NOT NULL",
    )
    .bind(query.session.as_str())
    .fetch_one(&mut **guard)
    .await
    .db()?;
    let sequence = u64::try_from(head.unwrap_or(0))
        .ok()
        .and_then(|value| acp_core::model::Sequence::new(value).ok())
        .ok_or_else(|| PortError::Corrupt("stored session sequence is out of range"))?;
    let read_cursor =
        acp_core::model::OriginCursor::new(decode(&epoch, "owned_session.origin_epoch")?, sequence);
    if messages.is_empty() {
        return Ok((read_cursor, Vec::new()));
    }
    // 取会话内全部可读 delta 行；`payload_json` 是 view 原文，JSON1 只用于定位，不改写正文。
    let rows = sqlx::query(
        "SELECT json_extract(payload_json, '$.messageId') AS mid, \
                CAST(json_extract(payload_json, '$.deltaIndex') AS INTEGER) AS idx \
           FROM owned_event \
          WHERE session_id = ?1 AND event_type = 'agent.message.delta' \
            AND session_sequence IS NOT NULL AND compacted_into IS NULL \
            AND json_extract(payload_json, '$.messageId') IS NOT NULL \
            AND json_extract(payload_json, '$.deltaIndex') IS NOT NULL \
          ORDER BY session_sequence ASC",
    )
    .bind(query.session.as_str())
    .fetch_all(&mut **guard)
    .await
    .db()?;
    let mut indexes: std::collections::BTreeMap<String, std::collections::BTreeSet<u64>> =
        std::collections::BTreeMap::new();
    for row in &rows {
        let mid: String = row.try_get("mid").db()?;
        let idx: i64 = row.try_get("idx").db()?;
        let idx =
            u64::try_from(idx).map_err(|_| PortError::Corrupt("stored delta index is negative"))?;
        indexes.entry(mid).or_default().insert(idx);
    }
    let mut coverage = Vec::with_capacity(messages.len());
    for message in messages {
        let mut last = None;
        if let Some(values) = indexes.get(message.message.as_str()) {
            for expected in 0..=values.len() as u64 {
                if values.contains(&expected) {
                    last = Some(expected);
                } else {
                    break;
                }
            }
        }
        coverage.push(acp_core::ports::MessageCoverage {
            message_id: message.message.clone(),
            through: read_cursor.clone(),
            last_delta_index: last,
        });
    }
    Ok((read_cursor, coverage))
}

// ---------------------------------------------------------------------------------------------
// AttachmentStore：内容寻址（§5.3/§7.3 的 owned_attachment*）
// ---------------------------------------------------------------------------------------------

/// 附件文件名 = 43 字符的规范 base64url 摘要，天然不含路径分隔符。
fn attachment_relative_path(digest: &Digest) -> String {
    format!("{}/{}", migrate::ATTACHMENTS_DIR, digest.as_str())
}

pub(crate) fn attachment_file_path(config: &StorageConfig, relative_path: &str) -> PathBuf {
    match relative_path.strip_prefix(&format!("{}/", migrate::ATTACHMENTS_DIR)) {
        Some(name) => config.attachment_dir.join(name),
        None => config.data_dir.join(relative_path),
    }
}

/// §7.1：原子创建 + 重命名（`SECURITY_DESIGN.md` §13.2 禁止可预测的共享临时路径）。
fn write_attachment_file(
    config: &StorageConfig,
    digest: &Digest,
    bytes: &[u8],
) -> Result<(), StorageError> {
    let final_path = attachment_file_path(config, &attachment_relative_path(digest));
    if final_path.exists() {
        // 内容寻址：同样的字节只存一份。
        return Ok(());
    }
    let temporary = config.attachment_dir.join(format!(".tmp-{}", new_uuid()));
    std::fs::write(&temporary, bytes)?;
    migrate::tighten_file_permissions(&temporary)?;
    match std::fs::rename(&temporary, &final_path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(StorageError::Io(error))
        }
    }
}

fn remove_attachment_files(config: &StorageConfig, relative_paths: &[String]) {
    for relative in relative_paths {
        let path = attachment_file_path(config, relative);
        let _ = std::fs::remove_file(path);
    }
}

#[async_trait]
impl AttachmentStore for SqliteStore {
    async fn put(
        &self,
        bytes: &[u8],
        media_type: &str,
        at: Timestamp,
    ) -> Result<AttachmentRef, PortError> {
        self.writable()?;
        let byte_length = u64::try_from(bytes.len())
            .map_err(|_| PortError::InvalidRequest("attachment is too large"))?;
        if byte_length > self.config.attachment_max_file_bytes {
            return Err(PortError::InvalidRequest(
                "attachment exceeds storage.attachment_max_file_bytes",
            ));
        }
        if media_type.is_empty() {
            return Err(PortError::InvalidRequest("attachment media type is empty"));
        }
        let digest_bytes = sha256(bytes);
        let digest = Digest::new(&{
            use base64::Engine as _;
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest_bytes)
        })
        .map_err(|_| PortError::InvalidRequest("computed digest is not canonical"))?;
        let relative_path = attachment_relative_path(&digest);
        if attachment_file_path(&self.config, &relative_path).exists() {
            // 已有同内容文件：不重写。
        } else {
            write_attachment_file(&self.config, &digest, bytes)?;
        }
        let mut tx = self.pools.write.begin_with("BEGIN IMMEDIATE").await.db()?;
        // 内容寻址：同一内容只保留**首次**落库的那一行（含它的 `attachment_id`），
        // 因此这里只刷新 `last_used_at`，并回读实际生效的 id。
        let generated = AttachmentId::new(&new_uuid())
            .map_err(|_| PortError::InvalidRequest("generated attachment id is malformed"))?;
        sqlx::query(
            "INSERT INTO owned_attachment (attachment_id, sha256, byte_length, media_type, \
             relative_path, created_at, last_used_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6) \
             ON CONFLICT(sha256) DO UPDATE SET last_used_at = excluded.last_used_at",
        )
        .bind(generated.as_str())
        .bind(digest.as_str())
        .bind(storable(byte_length)?)
        .bind(media_type)
        .bind(&relative_path)
        .bind(at.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        tx.commit().await.db()?;
        // §7.5：附件总量超预算时按 LRU 清理（含刚写入的这一条，如果它是最久未用的）。
        let over_budget = {
            let mut read = self.pools.read.acquire().await.db()?;
            measure_total(&mut read, &self.measure_sql).await? > self.config.max_total_size_bytes
        };
        if over_budget {
            let _ = self
                .prune_lru(self.config.attachment_max_total_bytes, at.clone())
                .await;
        }
        let stored_id: String =
            sqlx::query_scalar("SELECT attachment_id FROM owned_attachment WHERE sha256 = ?1")
                .bind(digest.as_str())
                .fetch_one(&self.pools.read)
                .await
                .db()?;
        Ok(AttachmentRef {
            id: decode(&stored_id, "owned_attachment.attachment_id")?,
            sha256: digest,
            byte_length,
            media_type: media_type.to_owned(),
            relative_path,
        })
    }

    async fn get(&self, id: &AttachmentId) -> Result<Option<Vec<u8>>, PortError> {
        let Some((digest, relative_path)) = self.resolve_attachment(id).await? else {
            return Ok(None);
        };
        let path = attachment_file_path(&self.config, &relative_path);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // 行在而文件不在：这是存储损坏（不是「未找到」），交由运维/启动检查处理。
                let _ = digest;
                Err(PortError::Corrupt("attachment bytes are missing"))
            }
            Err(_) => Err(PortError::Unavailable(UnavailableKind::IoError)),
        }
    }

    async fn link(
        &self,
        session: &SessionId,
        attachment: &AttachmentId,
        generation: AttachmentGeneration,
        at: Timestamp,
    ) -> Result<(), PortError> {
        self.writable()?;
        let (digest, _) = self
            .resolve_attachment(attachment)
            .await?
            .ok_or(PortError::InvalidRequest("attachment is not stored"))?;
        let mut tx = self.pools.write.begin_with("BEGIN IMMEDIATE").await.db()?;
        let session_exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM owned_session WHERE session_id = ?1")
                .bind(session.as_str())
                .fetch_optional(&mut *tx)
                .await
                .db()?;
        if session_exists.is_none() {
            return Err(PortError::NotFound(EntityRef::Session(session.clone())));
        }
        sqlx::query(
            "INSERT INTO owned_attachment_link (session_id, attachment_id, generation, sha256) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(session_id, attachment_id) DO UPDATE SET \
             generation = excluded.generation, sha256 = excluded.sha256",
        )
        .bind(session.as_str())
        .bind(attachment.as_str())
        .bind(storable(generation.get())?)
        .bind(digest.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        // 内容寻址去重让旧字节复用同一行（`created_at` 仍是首次上传的时刻），因此绑定必须把
        // 「最近一次使用」刷新到本次绑定的 `at`：`prune` 的链接清理与按引用 GC 都以它为界。
        sqlx::query("UPDATE owned_attachment SET last_used_at = ?1 WHERE attachment_id = ?2")
            .bind(at.as_str())
            .bind(attachment.as_str())
            .execute(&mut *tx)
            .await
            .db()?;
        tx.commit().await.db()?;
        Ok(())
    }

    /// §6 第 18 条 / §7.5：孤儿回收。
    ///
    /// 删除 `storage.attachment_dir` 下**不在 `owned_attachment` 里**且 `mtime < at` 的文件，最多 `limit`
    /// 个（`limit = 0` 不删）。表外但 `mtime >= at` 的文件必须保留（可能正在写、行还没提交）。
    ///
    /// 不读系统时钟：`at` 由调用方传入，只把它换算成 `SystemTime` 与文件 `mtime` 比较。文件删除在行事务
    /// 之外发生，失败也不阻止启动（调用方是组合根的启动流程）。目录条目按文件名排序，行为确定。
    async fn sweep_orphans(&self, at: Timestamp, limit: u32) -> Result<u32, PortError> {
        if limit == 0 {
            return Ok(0);
        }
        let at_millis = window::parse_millis(at.as_str()).ok_or(PortError::InvalidRequest(
            "timestamp is outside the supported window",
        ))?;
        let at_system = std::time::SystemTime::UNIX_EPOCH
            + std::time::Duration::from_millis(
                u64::try_from(at_millis)
                    .map_err(|_| PortError::InvalidRequest("timestamp is before the unix epoch"))?,
            );
        let tracked: std::collections::HashSet<String> =
            sqlx::query_scalar("SELECT relative_path FROM owned_attachment")
                .fetch_all(&self.pools.read)
                .await
                .db()?
                .into_iter()
                .collect();
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(&self.config.attachment_dir)
            .map_err(|_| PortError::Unavailable(UnavailableKind::IoError))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.is_file())
            .collect();
        entries.sort();
        let mut removed = 0_u32;
        for path in entries {
            if removed >= limit {
                break;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let relative = format!("{}/{}", migrate::ATTACHMENTS_DIR, name);
            if tracked.contains(&relative) {
                continue;
            }
            let modified = std::fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .map_err(|_| PortError::Unavailable(UnavailableKind::IoError))?;
            if modified >= at_system {
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => removed += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err(PortError::Unavailable(UnavailableKind::IoError)),
            }
        }
        Ok(removed)
    }

    async fn prune_lru(&self, budget_bytes: u64, at: Timestamp) -> Result<PruneReport, PortError> {
        self.writable()?;
        let mut tx = self.pools.write.begin_with("BEGIN IMMEDIATE").await.db()?;
        let (removed, freed, files) = sweep_attachments(&mut tx, budget_bytes, &at).await?;
        tx.commit().await.db()?;
        remove_attachment_files(&self.config, &files);
        let attached: i64 =
            sqlx::query_scalar("SELECT COALESCE(SUM(byte_length), 0) FROM owned_attachment")
                .fetch_one(&self.pools.read)
                .await
                .db()?;
        let still_over_limit = u64::try_from(attached).unwrap_or(0) > budget_bytes;
        Ok(PruneReport {
            removed_attachments: removed,
            freed_bytes: freed,
            still_over_limit,
            ..PruneReport::default()
        })
    }
}

impl SqliteStore {
    /// 由 `AttachmentId` 反查内容地址与相对路径。
    ///
    /// v0.3 的 §7.3 为 `owned_attachment` 增加了 `attachment_id` 列（由 `put` 分配并返回），因此这里是
    /// 一次按主键列的 O(1) 命中，不再需要任何按内容重算。
    async fn resolve_attachment(
        &self,
        id: &AttachmentId,
    ) -> Result<Option<(Digest, String)>, PortError> {
        let row = sqlx::query(
            "SELECT sha256, relative_path FROM owned_attachment WHERE attachment_id = ?1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pools.read)
        .await
        .db()?;
        match row {
            Some(row) => Ok(Some((
                decode(&text(&row, "sha256")?, "owned_attachment.sha256")?,
                text(&row, "relative_path")?,
            ))),
            None => Ok(None),
        }
    }
}
