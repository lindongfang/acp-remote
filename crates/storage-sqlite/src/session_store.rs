//! `SessionStore` / `ReadView` / `RemoteDeliveryStore` / `AttachmentStore` 的 SQLite 实现。
//!
//! 权威是 `docs/CORE_PORTS_AND_STORAGE.md`：§5.2 的端口形状、§6 的事务契约、§7 的表结构与 PRAGMA、
//! §7.5 的保留/清理/容量、§9 的验收判据。三条硬规则贯穿本文件：
//!
//! - **单写入口**：owned 家族只有 `SessionStore::commit` 会写 `owned_*`（状态 + turn + 事件 + 幂等记录 +
//!   命令终态在**同一事务**内），imported 家族只有 `RemoteDeliveryStore::commit_receipt` 会写交付索引；
//!   没有可分别调用的 repository/journal/deduper（`MODULE_ARCHITECTURE.md` §4.1）。
//! - **无正文**：`imported_*` 只写 §7.4 冻结的索引列，绝不写 prompt/回复/diff/终端/ACP raw。
//! - **不读系统时间**：所有时间来自参数（§2）；保留窗口的算术用 [`crate::migrate::window`] 的纯函数。
//!
//! 读列一律用显式列名（`row.try_get("列名")`），因此 Rust 侧字段顺序与表结构解耦；不用
//! `sqlx::FromRow` 派生是因为 workspace 的 `sqlx` features 没有 `macros`（不为此改依赖）。

use std::path::PathBuf;
use std::str::FromStr;

use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::{Row, SqliteConnection, Transaction};
use tokio::sync::Mutex;

use acp_core::model::{
    AcpRaw, Actor, ActorKind, AgentId, AgentRef, AttachmentGeneration, AttachmentId, CommandRecord,
    CommandResult, CommandStatus, CommittedDelivery, CommittedEvent, ConflictKind, Digest,
    EntityRef, EventId, EventKind, EventPayload, GlobalCursor, ImportId, InteractionOption,
    InteractionResolution, LocalCursor, ModeId, ModeRef, OriginCursor, OriginEpoch, OriginEventRef,
    PendingInteraction, PortError, PublicError, RawUnavailableReason, RemoteSessionRef, RequestId,
    ResourceOrigin, ScopeSet, Sequence, ServerEpoch, Session, SessionId, SessionSnapshot,
    SessionState, SessionSummary, StoredPolicy, Timestamp, Turn, TurnId, UnavailableKind, Version,
    ViewJson,
};
use acp_core::ports::{
    AckOutcome, AttachmentRef, AttachmentStore, CommitOutcome, DeliveryIndexEntry, DeliveryReceipt,
    DropReport, HistoryPage, HistoryQuery, IdempotentReplay, ImportedSessionQuery,
    ImportedSessionRecord, ModeChange, NewTurn, OwnedCommit, PruneReport, ReadView, ReceiptOutcome,
    RemoteCommandRef, RemoteDeliveryStore, ReplayBatch, ReplayLimit, ResetReason, RetentionPolicy,
    SessionAttachment, SessionQuery, SessionStore, StateChange, StoreHealth, TurnChange,
    TurnUpdate,
};

use crate::error::StorageError;
use crate::migrate::{self, Pools, StorageConfig, StoreMetadata, window};

/// `owned_event` 的读列（顺序无关，集中一处便于与 §7.3 对照）。
const EVENT_COLUMNS: &str = "global_sequence, session_id, session_sequence, origin_epoch, \
     origin_sequence, event_id, turn_id, event_type, kind, policy, origin_kind, causation, \
     payload_json, payload_digest, acp_media_type, acp_raw_json, acp_byte_length, acp_sha256, \
     acp_raw_unavailable_reason, created_at, expires_at, compacted_into";

/// `owned_session` 的读列。
const SESSION_COLUMNS: &str = "session_id, title, agent_id, agent_name, state, origin_epoch, \
     current_mode_id, current_mode_name, version, created_at, updated_at, closed_at";

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

    /// §7.1 失败关闭门：写路径的第一道检查。
    fn writable(&self) -> Result<(), PortError> {
        if self.integrity_ok {
            Ok(())
        } else {
            Err(PortError::Corrupt(
                "store is read-only after a failed integrity check",
            ))
        }
    }

    fn window(&self) -> RetentionPolicy {
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
/// `.db()` 收敛成 `StorageError`，再由 `?` 交给端口边界。
trait Db<T> {
    fn db(self) -> Result<T, StorageError>;
}

impl<T> Db<T> for Result<T, sqlx::Error> {
    fn db(self) -> Result<T, StorageError> {
        self.map_err(StorageError::from)
    }
}

fn text(row: &SqliteRow, column: &'static str) -> Result<String, StorageError> {
    row.try_get::<String, _>(column)
        .map_err(|_| StorageError::ColumnValue {
            column,
            expected: "non-null text",
        })
}

fn opt_text(row: &SqliteRow, column: &'static str) -> Result<Option<String>, StorageError> {
    row.try_get::<Option<String>, _>(column)
        .map_err(|_| StorageError::ColumnValue {
            column,
            expected: "text",
        })
}

fn int(row: &SqliteRow, column: &'static str) -> Result<i64, StorageError> {
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
fn decode<T>(value: &str, column: &'static str) -> Result<T, StorageError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value.parse::<T>().map_err(|_| StorageError::ColumnValue {
        column,
        expected: std::any::type_name::<T>(),
    })
}

fn decode_opt<T>(value: Option<String>, column: &'static str) -> Result<Option<T>, StorageError>
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

fn cursor(value: i64, column: &'static str) -> Result<LocalCursor, StorageError> {
    let value = u64::try_from(value).map_err(|_| StorageError::ColumnValue {
        column,
        expected: "non-negative local cursor",
    })?;
    Ok(LocalCursor::new(value))
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
fn actor_key(actor: &Actor) -> (ActorKind, String) {
    (actor.kind(), actor.id_text())
}

/// 从 `owned_command` 的 `(actor_kind, actor_id)` 还原 `Actor`。
///
/// `owned_command` 只存 §7.3 冻结的两列，因此还原是**有损**的：设备 scopes 不在表里（授权由 core 判定，
/// 存储不参与），这里给 `ScopeSet::empty()`；Node 的复合键按 `"{node}/{access_node}"` 拆回。
fn actor_from_columns(kind: ActorKind, id: &str) -> Result<Actor, StorageError> {
    match kind {
        ActorKind::Device => Ok(Actor::Device {
            device: decode(id, "owned_command.actor_id")?,
            scopes: ScopeSet::empty(),
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
        decode_opt(opt_text(row, "closed_at")?, "owned_session.closed_at")?,
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

/// 事件行的定位信息。
///
/// `payload_json`/`payload_digest`/`acp_*` 在 v1 的端口面上是**只写**的：`CommittedEvent`（以及
/// `HistoryPage.events`/`ReplayBatch.events` 里的 `CommittedDelivery::Owned`）只承载定位信息，不带
/// payload，而 §5.2 没有暴露任何「按事件读 payload」的方法。因此本适配器负责按 §7.3 的列与 §9.9 的
/// 摘要规则**写入**正文，读取端由后续增量补齐（见交付报告中的合同缺口）。
fn event_from_row(row: &SqliteRow) -> Result<CommittedEvent, StorageError> {
    let event = CommittedEvent {
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
const IMPORTED_SESSION_COLUMNS: &str = "owner_node_id, export_id, session_id, title, agent_id, \
     agent_name, state, version, created_at, last_origin_epoch, last_origin_sequence, \
     acked_origin_epoch, acked_origin_sequence, attachment_id, attachment_generation, updated_at";
const DELIVERY_INDEX_COLUMNS: &str = "owner_node_id, export_id, session_id, origin_event_id, \
     origin_epoch, origin_sequence, local_sequence, event_type, payload_digest, received_at";

const INSERT_EVENT: &str = "INSERT INTO owned_event (global_sequence, session_id, session_sequence, \
     origin_epoch, origin_sequence, event_id, turn_id, event_type, kind, policy, origin_kind, \
     causation, payload_json, payload_digest, acp_media_type, acp_raw_json, acp_byte_length, \
     acp_sha256, acp_raw_unavailable_reason, created_at, expires_at, compacted_into) \
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)";

const INSERT_COMMAND: &str = "INSERT INTO owned_command (actor_kind, actor_id, request_id, \
     session_id, command, kind, expected_version, request_fingerprint, accepted_at, status, \
     terminal_at, terminal_event_id, result_json, error_code, error_message, error_details_json, \
     retryable) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// §7.5 的容量度量：`SUM(length(payload_json)) + SUM(length(acp_raw_json)) +
/// SUM(owned_attachment.byte_length)`（不含 WAL/freelist）。
/// §7.5 的容量度量：`owned_*` 与 `imported_*` **两张族所有 TEXT 列**的 `length()` 之和，
/// 加上 `owned_attachment.byte_length` 之和（不含 WAL/freelist）。
///
/// 列清单在运行时从 `pragma_table_info` 取（只取 `type = 'TEXT'`），因此新增列自动计入：
/// Access-only 节点上 `owned_*` 恒为空，漏掉 `imported_*` 会让度量恒为 0，`RemoteDispatch::prune`
/// 的循环随即退出、`imported_*` 永不回收。
/// §7.5 的容量度量：事件正文（`payload_json` + `acp_raw_json`）之和 + `owned_attachment.byte_length` 之和。
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
        imported_schema_version: migrate::IMPORTED_SCHEMA_VERSION,
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
    let same = text(row, "command").map_err(PortError::from)? == idem.command
        && text(row, "kind").map_err(PortError::from)? == idem.kind.as_str()
        && stored_session.as_deref() == idem.session.as_ref().map(SessionId::as_str)
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
    async fn commit_owned(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
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
                // 命中路径不写任何行，显式结束事务。
                drop(tx);
                return Ok(CommitOutcome {
                    session_id: idem.session.clone(),
                    origin_epoch,
                    version,
                    appended: Vec::new(),
                    replayed: Some(IdempotentReplay { record }),
                });
            }
        }

        // ---- 命令**终态**提交：`command_terminal` 自身不带 requestId，靠同一提交里
        // `event_type ∈ {command.completed, command.failed, command.uncertain}` 且
        // `causation = requestId` 的事件定位既有 `owned_command` 行（§10.1/§11.2）。
        // 定位必须在写事件之前完成，否则重发的终态块会把事件重复追加一遍。
        let terminal_request = match (&commit.idempotency, &commit.command_terminal) {
            (None, Some(_)) => {
                let session = commit.session.clone().ok_or(PortError::InvalidRequest(
                    "a command terminal requires a session id",
                ))?;
                let request =
                    terminal_request_of(&commit.events).ok_or(PortError::InvalidRequest(
                        "a command terminal requires its terminal event with causation = requestId",
                    ))?;
                let rows = sqlx::query(&format!(
                    "SELECT {COMMAND_COLUMNS} FROM owned_command \
                     WHERE session_id = ?1 AND request_id = ?2 LIMIT 2"
                ))
                .bind(session.as_str())
                .bind(request.as_str())
                .fetch_all(&mut *tx)
                .await
                .db()?;
                match rows.len() {
                    0 => {
                        return Err(PortError::NotFound(EntityRef::Command {
                            session: Some(session),
                            request,
                        }));
                    }
                    1 => {
                        let row = &rows[0];
                        let status: CommandStatus =
                            decode(&text(row, "status")?, "owned_command.status")?;
                        if status != CommandStatus::Accepted {
                            // 同一终态块被重复投递：不追加事件、不改行，按重放回既有结果。
                            let record = command_record_from_row(row)?;
                            let (version, origin_epoch) =
                                match session_version_and_epoch(&mut tx, &session).await? {
                                    Some((version, epoch)) => (version, Some(epoch)),
                                    None => (Version::new(0), None),
                                };
                            drop(tx);
                            return Ok(CommitOutcome {
                                session_id: Some(session),
                                origin_epoch,
                                version,
                                appended: Vec::new(),
                                replayed: Some(IdempotentReplay { record }),
                            });
                        }
                        Some(request)
                    }
                    _ => {
                        return Err(PortError::InvalidRequest(
                            "request id is ambiguous within the session",
                        ));
                    }
                }
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
                let epoch = commit
                    .origin_epoch
                    .clone()
                    .ok_or(PortError::InvalidRequest(
                        "session creation requires an origin epoch",
                    ))?;
                let id_text = new_uuid();
                let id = SessionId::new(&id_text)
                    .map_err(|_| PortError::InvalidRequest("generated session id is malformed"))?;
                sqlx::query(
                    "INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, \
                     origin_epoch, current_mode_id, current_mode_name, version, created_at, \
                     updated_at, closed_at) \
                     VALUES (?1, ?2, ?3, ?4, 'idle', ?5, NULL, NULL, 1, ?6, ?6, NULL)",
                )
                .bind(&id_text)
                .bind(new.title.as_deref())
                .bind(new.agent.agent_id().as_str())
                .bind(new.agent.name())
                .bind(epoch.as_str())
                .bind(commit.at.as_str())
                .execute(&mut *tx)
                .await
                .db()?;
                session_id = Some(id);
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
                // `SessionUpdate.state` 是 `Option`：`None` = 不改状态（例如只解析一个交互）。
                // `closed_at` 为 `Some` 时会话进入 `Closed`；两者必须一致（`Session::try_new` 的不变量）。
                let effective_state = match (update.state, &update.closed_at) {
                    (_, Some(_)) => Some(SessionState::Closed),
                    (Some(SessionState::Closed), None) => {
                        return Err(PortError::InvalidRequest(
                            "closing a session requires closed_at",
                        ));
                    }
                    (Some(state), None) => Some(state),
                    (None, None) => None,
                };
                let (mode_id, mode_name) = mode_columns(&update.mode);
                // `CASE WHEN ?x IS NULL` 让「未提供的字段保持原值」，避免把已关闭会话的
                // `closed_at` 或既有状态写成 NULL。
                let statement = match &update.mode {
                    ModeChange::Unchanged => {
                        "UPDATE owned_session SET \
                         state = CASE WHEN ?1 IS NULL THEN state ELSE ?1 END, \
                         closed_at = CASE WHEN ?2 IS NULL THEN closed_at ELSE ?2 END, \
                         version = version + 1, updated_at = ?3 \
                         WHERE session_id = ?4 RETURNING version"
                    }
                    ModeChange::Set(_) => {
                        "UPDATE owned_session SET \
                         state = CASE WHEN ?1 IS NULL THEN state ELSE ?1 END, \
                         closed_at = CASE WHEN ?2 IS NULL THEN closed_at ELSE ?2 END, \
                         version = version + 1, updated_at = ?3, \
                         current_mode_id = ?5, current_mode_name = ?6 \
                         WHERE session_id = ?4 RETURNING version"
                    }
                };
                let mut query = sqlx::query_scalar::<_, i64>(statement)
                    .bind(effective_state.map(SessionState::as_str))
                    .bind(update.closed_at.as_ref().map(Timestamp::as_str))
                    .bind(commit.at.as_str())
                    .bind(session.as_str());
                if let ModeChange::Set(_) = &update.mode {
                    query = query.bind(mode_id).bind(mode_name);
                }
                let new_version: Option<i64> = query.fetch_optional(&mut *tx).await.db()?;
                version = match new_version {
                    Some(value) => parse_version(value, "owned_session.version")?,
                    None => return Err(PortError::NotFound(EntityRef::Session(session.clone()))),
                };
                // §5.2/§6.7：交互解析用条件更新实现 first-writer-wins，与状态修改同一事务。
                if let Some(resolved) = &update.interaction {
                    resolve_interaction(&mut tx, resolved, &commit.at).await?;
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

        // ---- 事件（会话级序号与全局序号在同一事务内分配）
        let mut appended = Vec::with_capacity(commit.events.len());
        let mut terminal_event_id: Option<EventId> = None;
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
                // 未知字段都不重排），所以只有规范化之后两边才算得一致（跨节点复算、imported 去重都依赖它）。
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
                if terminal_request.as_ref() == pending.causation.as_ref()
                    && TERMINAL_EVENT_TYPES.contains(&pending.event_type.as_str())
                {
                    terminal_event_id = Some(event_id.clone());
                }
                appended.push(CommittedEvent {
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
                 state, created_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
            )
            .bind(write.interaction.id().as_str())
            .bind(session.as_str())
            .bind(write.interaction.kind().as_str())
            .bind(storable(event_sequence.get())?)
            .bind(write.interaction.created_at().as_str())
            .execute(&mut *tx)
            .await
            .db()?;
        }

        // ---- 命令（幂等行 + 终态；§7.3 的 CHECK 形状由 `CommandRecord::try_new` 先行校验）
        if let Some(idem) = commit.idempotency.as_ref() {
            write_command(&mut tx, idem, commit.command_terminal.as_ref()).await?;
        }
        if let Some(request) = &terminal_request {
            let session = session_id.as_ref().ok_or(PortError::InvalidRequest(
                "a command terminal requires a session id",
            ))?;
            let terminal = commit
                .command_terminal
                .as_ref()
                .ok_or(PortError::InvalidRequest(
                    "a command terminal record is required",
                ))?;
            let event = terminal_event_id.as_ref().ok_or(PortError::InvalidRequest(
                "the terminal chunk must contain its command terminal event",
            ))?;
            apply_terminal(&mut tx, session, request, terminal, event).await?;
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

/// 把既有 `accepted` 行推进到终态（§11.2）。
///
/// `terminal_at`/`result`/`error` 为 `None` 表示**保持既有列不变**：终态块无法重发接受时的结果，
/// 用 `COALESCE` 而不是覆盖，避免把接受期写入的列清空。
async fn apply_terminal(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    session: &SessionId,
    request: &RequestId,
    terminal: &acp_core::model::CommandTerminalRecord,
    terminal_event: &EventId,
) -> Result<(), PortError> {
    let (error_code, error_message, error_details, retryable) = error_columns(terminal.error());
    let affected = sqlx::query(
        "UPDATE owned_command SET status = ?1, \
         terminal_at = COALESCE(?2, terminal_at), terminal_event_id = ?3, \
         result_json = COALESCE(?4, result_json), \
         error_code = COALESCE(?5, error_code), \
         error_message = COALESCE(?6, error_message), \
         error_details_json = COALESCE(?7, error_details_json), \
         retryable = COALESCE(?8, retryable) \
         WHERE session_id = ?9 AND request_id = ?10 AND status = 'accepted'",
    )
    .bind(terminal.status().as_str())
    .bind(terminal.terminal_at().map(Timestamp::as_str))
    .bind(terminal_event.as_str())
    .bind(terminal.result().map(|value| value.as_str()))
    .bind(error_code)
    .bind(error_message)
    .bind(error_details)
    .bind(retryable)
    .bind(session.as_str())
    .bind(request.as_str())
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
/// 条件更新（`state = 'pending'`）+ 受影响行数为 0 时**在同一事务内**回读：
/// 行存在且 `state <> 'pending'` → `Conflict(AlreadyResolved)`；无行 → `NotFound`。
/// 因此第二个应答不会覆盖既有结果（`decision_*`/`resolved_at` 保持不变）。
async fn resolve_interaction(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
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
    let affected = sqlx::query(
        "UPDATE owned_interaction SET state = 'resolved', resolved_at = ?1, \
         decision_option_id = ?2, decision_kind = ?3, elicitation_action = ?4, \
         elicitation_values_json = ?5, resolved_by_kind = ?6, resolved_by_id = ?7 \
         WHERE interaction_id = ?8 AND state = 'pending'",
    )
    .bind(at.as_str())
    .bind(decision_option_id)
    .bind(decision_kind)
    .bind(elicitation_action)
    .bind(elicitation_values)
    .bind(resolved_by_kind.as_str())
    .bind(resolved_by_id)
    .bind(resolved.interaction.as_str())
    .execute(&mut **tx)
    .await
    .db()?
    .rows_affected();
    if affected > 0 {
        return Ok(());
    }
    let state: Option<String> =
        sqlx::query_scalar("SELECT state FROM owned_interaction WHERE interaction_id = ?1")
            .bind(resolved.interaction.as_str())
            .fetch_optional(&mut **tx)
            .await
            .db()?;
    match state {
        Some(_) => Err(PortError::Conflict(ConflictKind::AlreadyResolved)),
        None => Err(PortError::NotFound(EntityRef::Interaction(
            resolved.interaction.clone(),
        ))),
    }
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
    freed_bytes: u64,
}

impl Sweep {
    fn add(&mut self, other: Sweep) {
        self.events += other.events;
        self.interactions += other.interactions;
        self.attachments += other.attachments;
        self.audit += other.audit;
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
/// 仍超限则拒绝本次写入。
///
/// §7.5 的顺序里 ④（附件 LRU）排在 ③ 之后，但附件字节在文件系统上：在本事务内删行而事务可能回滚，
/// 会造成「行还在、文件已删」的悬空行。因此容量清理在写事务内只做数据库侧（③①②），附件 LRU 由
/// `prune` / `AttachmentStore::prune_lru` 在事务提交后执行。若附件本身就把库顶到上限之上，本次提交会
/// 明确返回 `Unavailable(StorageFull)`，而不是静默丢弃附件或正文。
async fn enforce_capacity(
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
        "SELECT sha256, byte_length, relative_path FROM owned_attachment \
         ORDER BY last_used_at ASC, sha256 ASC",
    )
    .fetch_all(&mut *ex)
    .await
    .db()?;
    let mut removed = 0_u64;
    let mut freed = 0_u64;
    let mut files = Vec::new();
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
        files.push(relative);
    }
    Ok((removed, freed, files))
}

// ---------------------------------------------------------------------------------------------
// SessionStore / ReadView
// ---------------------------------------------------------------------------------------------

#[async_trait]
impl SessionStore for SqliteStore {
    async fn commit(&self, commit: OwnedCommit) -> Result<CommitOutcome, PortError> {
        self.writable()?;
        self.commit_owned(commit).await
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
        let mut sql = format!("SELECT {SESSION_COLUMNS} FROM owned_session");
        let mut clauses = Vec::new();
        let mut binds: Vec<String> = Vec::new();
        if let Some(only) = &query.only {
            clauses.push(format!("session_id IN ({})", placeholders(only.len())));
            binds.extend(only.iter().map(|id| id.as_str().to_owned()));
        }
        if !query.states.is_empty() {
            clauses.push(format!("state IN ({})", placeholders(query.states.len())));
            binds.extend(query.states.iter().map(|state| state.as_str().to_owned()));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY updated_at DESC, session_id ASC");
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

    /// §5.2：该会话**仍可重放**的 `session_sequence` 下界/上界；没有可重放行时返回 `None`。
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
        // ④ 附件 LRU（预算来自 §7.5 的 `storage.attachment_max_total_bytes`）
        let (removed, freed, files) =
            sweep_attachments(&mut tx, self.config.attachment_max_total_bytes).await?;
        sweep.attachments += removed;
        sweep.freed_bytes += freed;
        // ⑤ 到期审计
        let audit_threshold = threshold(&at, policy.audit_retention_days)?;
        sweep.audit = sqlx::query("DELETE FROM imported_audit WHERE at <= ?1")
            .bind(audit_threshold.as_str())
            .execute(&mut *tx)
            .await
            .db()?
            .rows_affected();
        sweep.audit += sqlx::query("DELETE FROM owned_audit WHERE at <= ?1")
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

        Ok(PruneReport {
            removed_events: sweep.events,
            removed_interactions: sweep.interactions,
            removed_attachments: sweep.attachments,
            removed_audit: sweep.audit,
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
            imported_schema_version: u32::try_from(self.meta.imported_schema_version).unwrap_or(0),
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
            "SELECT {EVENT_COLUMNS} FROM owned_event WHERE global_sequence > ?1 \
             ORDER BY global_sequence ASC LIMIT ?2"
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
            events.push(CommittedDelivery::Owned(event_from_row(row)?));
        }
        if rows.len() as u32 > limit.events()
            && let Some(last) = events.last()
            && let CommittedDelivery::Owned(event) = last
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
        let view = ViewJson::new(&text(&row, "payload_json")?)
            .map_err(|_| PortError::Corrupt("stored payload_json is not a JSON object"))?;
        let reason: Option<RawUnavailableReason> = decode_opt(
            opt_text(&row, "acp_raw_unavailable_reason")?,
            "owned_event.acp_raw_unavailable_reason",
        )?;
        let acp = match reason {
            Some(reason) => {
                let byte_length = match opt_int(&row, "acp_byte_length")? {
                    Some(value) => u64::try_from(value)
                        .map_err(|_| PortError::Corrupt("stored acp_byte_length is negative"))?,
                    None => 0,
                };
                let sha256 =
                    decode_opt::<Digest>(opt_text(&row, "acp_sha256")?, "owned_event.acp_sha256")?;
                Some(AcpRaw::unavailable(reason, byte_length, sha256))
            }
            None => match opt_text(&row, "acp_raw_json")? {
                Some(raw_json) => {
                    let media_type = text(&row, "acp_media_type")?;
                    let sha256 =
                        decode::<Digest>(&text(&row, "acp_sha256")?, "owned_event.acp_sha256")?;
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
        Ok(Some(payload))
    }

    async fn read_session(&self, query: HistoryQuery) -> Result<HistoryPage, PortError> {
        let mut guard = self.tx.lock().await;
        let head = head_of(&mut guard).await?;
        let row = sqlx::query(&format!(
            "SELECT {SESSION_COLUMNS} FROM owned_session WHERE session_id = ?1"
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
        // `messages` 是历史页的正文开关（§11.5 的 `include`）；关掉时只返回非正文的事件元数据。
        let events = if query.include.messages {
            let rows = sqlx::query(&format!(
                "SELECT {EVENT_COLUMNS} FROM owned_event WHERE session_id = ?1 \
                 AND global_sequence > ?2 ORDER BY global_sequence ASC LIMIT ?3"
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
                events.push(event_from_row(row)?);
            }
            events
        } else {
            Vec::new()
        };
        let turns = if query.include.turns {
            let rows = sqlx::query(&format!(
                "SELECT {TURN_COLUMNS} FROM owned_turn WHERE session_id = ?1 ORDER BY queue_index ASC"
            ))
            .bind(query.session.as_str())
            .fetch_all(&mut **guard)
            .await.db()?;
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
                 AND state = 'pending' ORDER BY created_at ASC, interaction_id ASC"
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
            if last.global_sequence.get() < head.global_sequence.get() {
                Some(GlobalCursor::new(
                    self.server_epoch.clone(),
                    last.global_sequence,
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
            turns,
            interactions,
            // 活体数据由用例层合并（`HistoryPage` 的文档注释）：持久层不保存 config/capabilities。
            config: Vec::new(),
            capabilities: None,
            head,
            next,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// RemoteDeliveryStore：imported 家族的唯一写入口，且**无正文**（§7.4）
// ---------------------------------------------------------------------------------------------

#[async_trait]
impl RemoteDeliveryStore for SqliteStore {
    async fn commit_receipt(&self, receipt: DeliveryReceipt) -> Result<ReceiptOutcome, PortError> {
        self.writable()?;
        let mut tx = self.pools.write.begin_with("BEGIN IMMEDIATE").await.db()?;
        let session = &receipt.session;
        let next: Option<i64> = sqlx::query_scalar(
            "SELECT next_local_sequence FROM imported_session \
             WHERE owner_node_id = ?1 AND export_id = ?2 AND session_id = ?3",
        )
        .bind(session.owner_node_id.as_str())
        .bind(session.export_id.as_str())
        .bind(session.session_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?;
        let session_next = next
            .ok_or_else(|| PortError::NotFound(EntityRef::Session(session.session_id.clone())))?;

        // §9.5：同一 `origin_event_id` 重发只产生一行、不新增 `local_sequence`。
        let duplicate: Option<i64> = sqlx::query_scalar(
            "SELECT local_sequence FROM imported_delivery_index \
             WHERE owner_node_id = ?1 AND export_id = ?2 AND session_id = ?3 AND origin_event_id = ?4",
        )
        .bind(session.owner_node_id.as_str())
        .bind(session.export_id.as_str())
        .bind(session.session_id.as_str())
        .bind(receipt.origin.origin_event_id.as_str())
        .fetch_optional(&mut *tx)
        .await.db()?;
        if let Some(local) = duplicate {
            return Ok(ReceiptOutcome {
                local_sequence: cursor(local, "imported_delivery_index.local_sequence")?,
                duplicate: true,
            });
        }

        // `local_sequence` 是**全库单调**的投递序号：`local_replay(after)` 只有一个游标参数，
        // 因此序号必须跨会话可比。分配时取「全库最大值 + 1」，同时把该会话的
        // `next_local_sequence` 推进到同一水位（§7.4 的列语义保持一致）。
        let global_next: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(local_sequence), 0) + 1 FROM imported_delivery_index",
        )
        .fetch_one(&mut *tx)
        .await
        .db()?;
        let allocated = global_next.max(session_next);
        sqlx::query(
            "INSERT INTO imported_delivery_index (owner_node_id, export_id, session_id, \
             origin_event_id, origin_epoch, origin_sequence, local_sequence, event_type, \
             payload_digest, received_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(session.owner_node_id.as_str())
        .bind(session.export_id.as_str())
        .bind(session.session_id.as_str())
        .bind(receipt.origin.origin_event_id.as_str())
        .bind(receipt.origin.origin_epoch.as_str())
        .bind(storable(receipt.origin_sequence.get())?)
        .bind(allocated)
        .bind(receipt.event_type.as_str())
        .bind(receipt.payload_digest.as_str())
        .bind(receipt.at.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        sqlx::query(
            "UPDATE imported_session SET next_local_sequence = ?1, last_origin_epoch = ?2, \
             last_origin_sequence = CASE WHEN last_origin_sequence IS NULL OR \
             last_origin_sequence < ?3 THEN ?3 ELSE last_origin_sequence END, updated_at = ?4 \
             WHERE owner_node_id = ?5 AND export_id = ?6 AND session_id = ?7",
        )
        .bind(allocated + 1)
        .bind(receipt.origin.origin_epoch.as_str())
        .bind(storable(receipt.origin_sequence.get())?)
        .bind(receipt.at.as_str())
        .bind(session.owner_node_id.as_str())
        .bind(session.export_id.as_str())
        .bind(session.session_id.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        tx.commit().await.db()?;
        Ok(ReceiptOutcome {
            local_sequence: cursor(allocated, "imported_delivery_index.local_sequence")?,
            duplicate: false,
        })
    }

    async fn local_replay(
        &self,
        after: Option<LocalCursor>,
        limit: ReplayLimit,
    ) -> Result<Vec<DeliveryIndexEntry>, PortError> {
        let from = after.map_or(0, |cursor| cursor.get());
        let rows = sqlx::query(&format!(
            "SELECT {DELIVERY_INDEX_COLUMNS} FROM imported_delivery_index \
             WHERE local_sequence > ?1 ORDER BY local_sequence ASC LIMIT ?2"
        ))
        .bind(i64::try_from(from).unwrap_or(i64::MAX))
        .bind(i64::from(limit.events()))
        .fetch_all(&self.pools.read)
        .await
        .db()?;
        let mut entries = Vec::with_capacity(rows.len());
        for row in &rows {
            entries.push(delivery_index_from_row(row)?);
        }
        Ok(entries)
    }

    async fn ack(
        &self,
        session: &RemoteSessionRef,
        cursor_value: OriginCursor,
        at: Timestamp,
    ) -> Result<AckOutcome, PortError> {
        self.writable()?;
        let affected = sqlx::query(
            "UPDATE imported_session SET acked_origin_epoch = ?1, acked_origin_sequence = ?2, \
             updated_at = ?3 WHERE owner_node_id = ?4 AND export_id = ?5 AND session_id = ?6 \
             AND (acked_origin_epoch IS NULL OR acked_origin_epoch <> ?1 \
                  OR acked_origin_sequence IS NULL OR acked_origin_sequence < ?2)",
        )
        .bind(cursor_value.origin_epoch.as_str())
        .bind(storable(cursor_value.origin_sequence.get())?)
        .bind(at.as_str())
        .bind(session.owner_node_id.as_str())
        .bind(session.export_id.as_str())
        .bind(session.session_id.as_str())
        .execute(&self.pools.write)
        .await
        .db()?
        .rows_affected();
        if affected > 0 {
            return Ok(AckOutcome {
                applied: true,
                cursor: Some(cursor_value),
            });
        }
        // 回退请求（或会话不存在）：保持既有游标。
        let existing = self.load_ack(session).await?;
        if existing.is_none() {
            let present: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM imported_session WHERE owner_node_id = ?1 AND export_id = ?2 \
                 AND session_id = ?3",
            )
            .bind(session.owner_node_id.as_str())
            .bind(session.export_id.as_str())
            .bind(session.session_id.as_str())
            .fetch_optional(&self.pools.read)
            .await
            .db()?;
            if present.is_none() {
                return Err(PortError::NotFound(EntityRef::Session(
                    session.session_id.clone(),
                )));
            }
        }
        Ok(AckOutcome {
            applied: false,
            cursor: existing,
        })
    }

    async fn load_ack(
        &self,
        session: &RemoteSessionRef,
    ) -> Result<Option<OriginCursor>, PortError> {
        let row = sqlx::query(
            "SELECT acked_origin_epoch, acked_origin_sequence FROM imported_session \
             WHERE owner_node_id = ?1 AND export_id = ?2 AND session_id = ?3",
        )
        .bind(session.owner_node_id.as_str())
        .bind(session.export_id.as_str())
        .bind(session.session_id.as_str())
        .fetch_optional(&self.pools.read)
        .await
        .db()?;
        let Some(row) = row else {
            return Ok(None);
        };
        match (
            opt_text(&row, "acked_origin_epoch")?,
            opt_int(&row, "acked_origin_sequence")?,
        ) {
            (Some(epoch), Some(sequence)) => Ok(Some(OriginCursor::new(
                decode(&epoch, "imported_session.acked_origin_epoch")?,
                sequence_from(sequence, "imported_session.acked_origin_sequence")?,
            ))),
            _ => Ok(None),
        }
    }

    async fn upsert_session(
        &self,
        record: ImportedSessionRecord,
        at: Timestamp,
    ) -> Result<(), PortError> {
        self.writable()?;
        let mut tx = self.pools.write.begin_with("BEGIN IMMEDIATE").await.db()?;
        let session = &record.session;
        let stored_epoch: Option<String> = sqlx::query_scalar(
            "SELECT last_origin_epoch FROM imported_session \
             WHERE owner_node_id = ?1 AND export_id = ?2 AND session_id = ?3",
        )
        .bind(session.owner_node_id.as_str())
        .bind(session.export_id.as_str())
        .bind(session.session_id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?;
        let (agent_id, agent_name) = match &record.agent {
            Some(agent) => (
                Some(agent.agent_id().as_str().to_owned()),
                Some(agent.name().to_owned()),
            ),
            None => (None, None),
        };
        sqlx::query(
            "INSERT INTO imported_session (owner_node_id, export_id, session_id, title, agent_id, \
             agent_name, state, version, created_at, last_origin_epoch, last_origin_sequence, \
             acked_origin_epoch, acked_origin_sequence, next_local_sequence, attachment_id, \
             attachment_generation, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14, ?15, ?16) \
             ON CONFLICT(owner_node_id, export_id, session_id) DO UPDATE SET \
             title = excluded.title, agent_id = excluded.agent_id, agent_name = excluded.agent_name, \
             state = excluded.state, version = excluded.version, created_at = excluded.created_at, \
             last_origin_epoch = excluded.last_origin_epoch, \
             last_origin_sequence = excluded.last_origin_sequence, \
             acked_origin_epoch = excluded.acked_origin_epoch, \
             acked_origin_sequence = excluded.acked_origin_sequence, \
             attachment_id = excluded.attachment_id, \
             attachment_generation = excluded.attachment_generation, \
             updated_at = excluded.updated_at",
        )
        .bind(session.owner_node_id.as_str())
        .bind(session.export_id.as_str())
        .bind(session.session_id.as_str())
        .bind(record.title.as_deref())
        .bind(agent_id)
        .bind(agent_name)
        .bind(record.state.map(|state| state.as_str()))
        .bind(
            record
                .version
                .map(|value| storable(value.get()))
                .transpose()?,
        )
        .bind(record.created_at.as_ref().map(Timestamp::as_str))
        .bind(record.origin_epoch.as_ref().map(OriginEpoch::as_str))
        .bind(
            record
                .last_origin_sequence
                .map(|value| storable(value.get()))
                .transpose()?,
        )
        .bind(record.acked.as_ref().map(|cursor| cursor.origin_epoch.as_str()))
        .bind(
            record
                .acked
                .as_ref()
                .map(|cursor| storable(cursor.origin_sequence.get()))
                .transpose()?,
        )
        .bind(record.attachment.as_ref().map(|value| value.id.as_str()))
        .bind(
            record
                .attachment
                .as_ref()
                .map(|value| storable(value.generation.get()))
                .transpose()?,
        )
        .bind(at.as_str())
        .execute(&mut *tx)
        .await.db()?;

        // §7.4：owner 的 epoch 变了说明对方重建了事件库，该会话的 cursor 全部失效。
        if let Some(stored) = stored_epoch
            && record
                .origin_epoch
                .as_ref()
                .is_some_and(|epoch| epoch.as_str() != stored)
        {
            sqlx::query(
                "DELETE FROM imported_delivery_index WHERE owner_node_id = ?1 AND export_id = ?2 \
                 AND session_id = ?3",
            )
            .bind(session.owner_node_id.as_str())
            .bind(session.export_id.as_str())
            .bind(session.session_id.as_str())
            .execute(&mut *tx)
            .await
            .db()?;
            sqlx::query(
                "UPDATE imported_session SET acked_origin_epoch = NULL, \
                 acked_origin_sequence = NULL WHERE owner_node_id = ?1 AND export_id = ?2 \
                 AND session_id = ?3",
            )
            .bind(session.owner_node_id.as_str())
            .bind(session.export_id.as_str())
            .bind(session.session_id.as_str())
            .execute(&mut *tx)
            .await
            .db()?;
        }
        tx.commit().await.db()?;
        Ok(())
    }

    async fn list_sessions(
        &self,
        query: ImportedSessionQuery,
    ) -> Result<Vec<ImportedSessionRecord>, PortError> {
        if query.imports.as_ref().is_some_and(|list| list.is_empty())
            || query.only.as_ref().is_some_and(|list| list.is_empty())
        {
            return Ok(Vec::new());
        }
        if query.limit == Some(0) {
            return Ok(Vec::new());
        }
        // `only` 是按主键的精确匹配：逐个取回，再统一截断到 `limit`。
        if let Some(only) = &query.only {
            let mut records = Vec::with_capacity(only.len());
            for reference in only {
                let row = sqlx::query(&format!(
                    "SELECT {IMPORTED_SESSION_COLUMNS} FROM imported_session \
                     WHERE owner_node_id = ?1 AND export_id = ?2 AND session_id = ?3"
                ))
                .bind(reference.owner_node_id.as_str())
                .bind(reference.export_id.as_str())
                .bind(reference.session_id.as_str())
                .fetch_optional(&self.pools.read)
                .await
                .db()?;
                if let Some(row) = row
                    && self.import_matches(&row, query.imports.as_deref()).await?
                {
                    records.push(imported_session_from_row(&row)?);
                }
                if query
                    .limit
                    .is_some_and(|limit| records.len() >= limit as usize)
                {
                    break;
                }
            }
            return Ok(records);
        }
        let mut sql = format!("SELECT {IMPORTED_SESSION_COLUMNS} FROM imported_session");
        let mut binds: Vec<String> = Vec::new();
        if let Some(imports) = &query.imports {
            sql.push_str(&format!(
                " WHERE (owner_node_id, export_id) IN (SELECT owner_node_id, export_id \
                 FROM imported_import WHERE import_id IN ({}))",
                placeholders(imports.len())
            ));
            binds.extend(imports.iter().map(|id| id.as_str().to_owned()));
        }
        sql.push_str(" ORDER BY updated_at DESC, owner_node_id ASC, export_id ASC, session_id ASC");
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        let mut statement = sqlx::query(&sql);
        for value in &binds {
            statement = statement.bind(value.as_str());
        }
        let rows = statement.fetch_all(&self.pools.read).await.db()?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            records.push(imported_session_from_row(row)?);
        }
        Ok(records)
    }

    async fn find_remote_request(
        &self,
        session: &RemoteSessionRef,
        request: &RequestId,
    ) -> Result<Option<RemoteCommandRef>, PortError> {
        let row = sqlx::query(
            "SELECT request_id, command, status, accepted_at, terminal_at, terminal_event_id, \
             error_code, retryable FROM imported_command_ref \
             WHERE owner_node_id = ?1 AND export_id = ?2 AND session_id = ?3 AND request_id = ?4",
        )
        .bind(session.owner_node_id.as_str())
        .bind(session.export_id.as_str())
        .bind(session.session_id.as_str())
        .bind(request.as_str())
        .fetch_optional(&self.pools.read)
        .await
        .db()?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(remote_command_from_row(&row)?))
    }

    async fn drop_import(&self, import: &ImportId) -> Result<DropReport, PortError> {
        self.writable()?;
        let mut tx = self.pools.write.begin_with("BEGIN IMMEDIATE").await.db()?;
        let owner = sqlx::query(
            "SELECT owner_node_id, export_id FROM imported_import WHERE import_id = ?1",
        )
        .bind(import.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?;
        let Some(owner) = owner else {
            return Err(PortError::NotFound(EntityRef::Import(import.clone())));
        };
        let owner_node_id = text(&owner, "owner_node_id")?;
        let export_id = text(&owner, "export_id")?;
        // §5.2：只删交付索引与命令引用；`imported_audit` 保留（`SECURITY_DESIGN.md` §11.5）。
        let delivery_index_removed = sqlx::query(
            "DELETE FROM imported_delivery_index WHERE owner_node_id = ?1 AND export_id = ?2",
        )
        .bind(&owner_node_id)
        .bind(&export_id)
        .execute(&mut *tx)
        .await
        .db()?
        .rows_affected();
        let command_refs_removed = sqlx::query(
            "DELETE FROM imported_command_ref WHERE owner_node_id = ?1 AND export_id = ?2",
        )
        .bind(&owner_node_id)
        .bind(&export_id)
        .execute(&mut *tx)
        .await
        .db()?
        .rows_affected();
        sqlx::query("UPDATE imported_import SET removed_at = ?1 WHERE import_id = ?2")
            .bind(owner_node_id)
            .bind(import.as_str())
            .execute(&mut *tx)
            .await
            .db()?;
        tx.commit().await.db()?;
        Ok(DropReport {
            delivery_index_removed,
            command_refs_removed,
        })
    }

    /// §7.5 的容量清理对 imported 家族的对应物：交付索引与命令引用是**索引元数据**（无正文），
    /// 没有按内容的 TTL；超限时按 `local_sequence` 从最旧开始删（删除后客户端会收到
    /// `cursor_expired` 并重建快照），仍超限则报告 `still_over_limit`。
    async fn prune(
        &self,
        policy: RetentionPolicy,
        at: Timestamp,
    ) -> Result<PruneReport, PortError> {
        self.writable()?;
        let mut tx = self.pools.write.begin_with("BEGIN IMMEDIATE").await.db()?;
        // ⑤ TTL：到期审计（与 `SessionStore::prune` 同一步骤）。**不**受容量是否超限影响——它由
        // `prune` 驱动，提前返回会让 Access-only 节点的 `imported_audit` 永远不清。
        let audit_threshold = threshold(&at, policy.audit_retention_days)?;
        let removed_audit = sqlx::query("DELETE FROM imported_audit WHERE at <= ?1")
            .bind(audit_threshold.as_str())
            .execute(&mut *tx)
            .await
            .db()?
            .rows_affected();
        // 容量：只在超限时按 `local_sequence` 升序回收交付索引（阈值以下时循环体不执行）。
        let mut removed = 0_u64;
        while measure_total(&mut tx, &self.measure_sql).await? > policy.max_total_size_bytes {
            let rows = sqlx::query(
                "SELECT rowid AS row_id FROM imported_delivery_index \
                 ORDER BY local_sequence ASC LIMIT 64",
            )
            .fetch_all(&mut *tx)
            .await
            .db()?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                let row_id = int(&row, "row_id")?;
                removed += sqlx::query("DELETE FROM imported_delivery_index WHERE rowid = ?1")
                    .bind(row_id)
                    .execute(&mut *tx)
                    .await
                    .db()?
                    .rows_affected();
            }
        }
        let still_over_limit =
            measure_total(&mut tx, &self.measure_sql).await? > policy.max_total_size_bytes;
        sqlx::query("UPDATE meta SET value = ?1 WHERE key = 'last_prune_at'")
            .bind(at.as_str())
            .execute(&mut *tx)
            .await
            .db()?;
        tx.commit().await.db()?;
        Ok(PruneReport {
            removed_events: removed,
            removed_audit,
            still_over_limit,
            ..PruneReport::default()
        })
    }
}

impl SqliteStore {
    /// `list_sessions` 的 `imports` 过滤：`Some([])` 已在入口处理，这里只判成员资格。
    async fn import_matches(
        &self,
        row: &SqliteRow,
        imports: Option<&[ImportId]>,
    ) -> Result<bool, StorageError> {
        let Some(imports) = imports else {
            return Ok(true);
        };
        let owner = text(row, "owner_node_id")?;
        let export = text(row, "export_id")?;
        let matched: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM imported_import WHERE owner_node_id = ?1 AND export_id = ?2 \
             AND import_id IN (SELECT value FROM json_each(?3))",
        )
        .bind(&owner)
        .bind(&export)
        .bind(
            imports
                .iter()
                .map(|id| format!("\"{}\"", id.as_str()))
                .collect::<Vec<_>>()
                .join(","),
        )
        .fetch_optional(&self.pools.read)
        .await?;
        Ok(matched.is_some())
    }
}

fn sequence_from(value: i64, column: &'static str) -> Result<Sequence, StorageError> {
    sequence(value, column)
}

fn delivery_index_from_row(row: &SqliteRow) -> Result<DeliveryIndexEntry, StorageError> {
    Ok(DeliveryIndexEntry {
        session: RemoteSessionRef::new(
            decode(
                &text(row, "owner_node_id")?,
                "imported_delivery_index.owner_node_id",
            )?,
            decode(
                &text(row, "export_id")?,
                "imported_delivery_index.export_id",
            )?,
            decode(
                &text(row, "session_id")?,
                "imported_delivery_index.session_id",
            )?,
        ),
        origin: OriginEventRef::new(
            decode(
                &text(row, "owner_node_id")?,
                "imported_delivery_index.owner_node_id",
            )?,
            decode(
                &text(row, "origin_epoch")?,
                "imported_delivery_index.origin_epoch",
            )?,
            decode(
                &text(row, "origin_event_id")?,
                "imported_delivery_index.origin_event_id",
            )?,
        ),
        origin_sequence: sequence(
            int(row, "origin_sequence")?,
            "imported_delivery_index.origin_sequence",
        )?,
        local_sequence: cursor(
            int(row, "local_sequence")?,
            "imported_delivery_index.local_sequence",
        )?,
        event_type: decode(
            &text(row, "event_type")?,
            "imported_delivery_index.event_type",
        )?,
        payload_digest: decode(
            &text(row, "payload_digest")?,
            "imported_delivery_index.payload_digest",
        )?,
        received_at: decode(
            &text(row, "received_at")?,
            "imported_delivery_index.received_at",
        )?,
    })
}

fn imported_session_from_row(row: &SqliteRow) -> Result<ImportedSessionRecord, StorageError> {
    let owner_node_id = decode(
        &text(row, "owner_node_id")?,
        "imported_session.owner_node_id",
    )?;
    let export_id = decode(&text(row, "export_id")?, "imported_session.export_id")?;
    let session_id = decode(&text(row, "session_id")?, "imported_session.session_id")?;
    let agent = match (opt_text(row, "agent_id")?, opt_text(row, "agent_name")?) {
        (Some(id), Some(name)) => {
            let id = AgentId::new(&id).map_err(|_| StorageError::ColumnValue {
                column: "imported_session.agent_id",
                expected: "agent id",
            })?;
            Some(
                AgentRef::try_new(id, &name).map_err(|_| StorageError::ColumnValue {
                    column: "imported_session.agent_name",
                    expected: "agent name",
                })?,
            )
        }
        _ => None,
    };
    let last_origin_sequence = match opt_int(row, "last_origin_sequence")? {
        Some(value) => Some(sequence(value, "imported_session.last_origin_sequence")?),
        None => None,
    };
    let acked = match (
        opt_text(row, "acked_origin_epoch")?,
        opt_int(row, "acked_origin_sequence")?,
    ) {
        (Some(epoch), Some(sequence)) => Some(OriginCursor::new(
            decode(&epoch, "imported_session.acked_origin_epoch")?,
            sequence_from(sequence, "imported_session.acked_origin_sequence")?,
        )),
        _ => None,
    };
    let attachment = match (
        opt_text(row, "attachment_id")?,
        opt_int(row, "attachment_generation")?,
    ) {
        (Some(id), Some(generation)) => Some(SessionAttachment {
            id: decode(&id, "imported_session.attachment_id")?,
            generation: AttachmentGeneration::new(u64::try_from(generation).unwrap_or(0)),
        }),
        _ => None,
    };
    Ok(ImportedSessionRecord {
        session: RemoteSessionRef::new(owner_node_id, export_id, session_id),
        origin_epoch: decode_opt(
            opt_text(row, "last_origin_epoch")?,
            "imported_session.last_origin_epoch",
        )?,
        title: opt_text(row, "title")?,
        agent,
        state: decode_opt(opt_text(row, "state")?, "imported_session.state")?,
        version: match opt_int(row, "version")? {
            Some(value) => Some(parse_version(value, "imported_session.version")?),
            None => None,
        },
        created_at: decode_opt(opt_text(row, "created_at")?, "imported_session.created_at")?,
        last_origin_sequence,
        acked,
        attachment,
        updated_at: decode(&text(row, "updated_at")?, "imported_session.updated_at")?,
    })
}

fn remote_command_from_row(row: &SqliteRow) -> Result<RemoteCommandRef, StorageError> {
    // §7.4：imported 家族不存正文，`error_message`/`details` 不在表里（黄金列清单），因此这里只还原
    // 错误码与可重试标志；完整错误由 Access 在线向 Owner 回源。
    let error = match opt_text(row, "error_code")? {
        Some(code) => Some(
            PublicError::try_new(
                &code,
                "",
                opt_flag(row, "retryable")?.unwrap_or(false),
                ViewJson::empty_object(),
            )
            .map_err(|_| StorageError::ColumnValue {
                column: "imported_command_ref.error_code",
                expected: "error code",
            })?,
        ),
        None => None,
    };
    Ok(RemoteCommandRef {
        request: decode(&text(row, "request_id")?, "imported_command_ref.request_id")?,
        command: text(row, "command")?,
        status: decode(&text(row, "status")?, "imported_command_ref.status")?,
        accepted_at: decode_opt(
            opt_text(row, "accepted_at")?,
            "imported_command_ref.accepted_at",
        )?,
        terminal_at: decode_opt(
            opt_text(row, "terminal_at")?,
            "imported_command_ref.terminal_at",
        )?,
        terminal_event: decode_opt(
            opt_text(row, "terminal_event_id")?,
            "imported_command_ref.terminal_event_id",
        )?,
        error,
    })
}

// ---------------------------------------------------------------------------------------------
// AttachmentStore：内容寻址（§5.3/§7.3 的 owned_attachment*）
// ---------------------------------------------------------------------------------------------

/// 附件文件名 = 43 字符的规范 base64url 摘要，天然不含路径分隔符。
fn attachment_relative_path(digest: &Digest) -> String {
    format!("{}/{}", migrate::ATTACHMENTS_DIR, digest.as_str())
}

fn attachment_file_path(config: &StorageConfig, relative_path: &str) -> PathBuf {
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
        tx.commit().await.db()?;
        Ok(())
    }

    async fn prune_lru(&self, budget_bytes: u64, at: Timestamp) -> Result<PruneReport, PortError> {
        self.writable()?;
        let _ = at;
        let mut tx = self.pools.write.begin_with("BEGIN IMMEDIATE").await.db()?;
        let (removed, freed, files) = sweep_attachments(&mut tx, budget_bytes).await?;
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
