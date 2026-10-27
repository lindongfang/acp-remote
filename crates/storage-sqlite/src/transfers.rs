//! 分块附件传输、提交清单与有效待处理交互的 SQLite 实现
//! （`docs/CORE_PORTS_AND_STORAGE.md` §5.8/§6 第 22 条/§7.6）。
//!
//! 三条硬边界：
//!
//! - **内存有界**：本模块从不把整段附件读进内存。`append_chunk` 只接受单块 ≤32 KiB，
//!   `seal_transfer` 以 32 KiB 为步长做流式 SHA-256，`read_chunk` 每次最多返回一块。
//! - **事务与文件的顺序**：临时文件先写、校验通过后 rename，**再**提交引用行；删除一律在事务提交
//!   之后（崩溃最坏留下无人引用的孤儿文件，绝不会留下悬空行）。
//! - **交互阶段**：`effective` 只读 `pending` 未失效行；仲裁（`pending -> resolved`）与确认
//!   （`resolved + confirmed_at IS NULL -> confirmed_at`）都走 `SessionStore::commit`，因此首答胜出、
//!   确认与会话摘要永远在同一事务里收敛。本模块只保留读与批量失效。

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;

use acp_core::model::{
    AttachmentId, AttachmentSubject, AttachmentSubmission, AttachmentTransfer,
    AttachmentTransferState, ConflictKind, Digest, EffectiveInteraction, ElicitationSchema,
    EntityRef, InteractionId, InteractionKind, InteractionOption, PortError, SealedAttachment,
    SessionId, SubmissionId, Timestamp, UnavailableKind, Version,
};
use acp_core::ports::{
    AttachmentChunk, AttachmentRead, AttachmentTransferStore, ChunkOutcome, InteractionAuthority,
    SubmissionBegin, SubmissionPage, SubmissionProgress, SubmissionSeal, TransferBegin,
    TransferChunk, TransferSeal,
};

use crate::error::StorageError;
use crate::migrate::{self, StorageConfig, window};
use crate::session_store::{Db, SqliteStore, int, opt_text, text};

/// 附件传输的固定单块上限（与 core 的常量同值；wire 的 `chunkSize` 也是它）。
const CHUNK_SIZE: usize = 32_768;
/// 闲置清理窗口：24 小时。
const IDLE_HOURS_DAYS: i64 = -1;

fn stale_threshold(at: &Timestamp) -> Result<Timestamp, PortError> {
    let shifted = window::shift_days(at.as_str(), IDLE_HOURS_DAYS).ok_or(
        PortError::InvalidRequest("timestamp is outside the supported window"),
    )?;
    Timestamp::new(&shifted).map_err(|_| PortError::InvalidRequest("timestamp shape"))
}

fn storable(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Unavailable(UnavailableKind::StorageFull))
}

fn subject_columns(subject: &AttachmentSubject) -> (&'static str, &str) {
    (subject.kind(), subject.id_text())
}

fn subject_from_row(row: &SqliteRow) -> Result<AttachmentSubject, StorageError> {
    let kind = text(row, "subject_kind")?;
    let id = text(row, "subject_id")?;
    match kind.as_str() {
        "device" => acp_core::model::DeviceId::new(&id)
            .map(AttachmentSubject::Device)
            .map_err(|_| StorageError::ColumnValue {
                column: "owned_attachment_transfer.subject_id",
                expected: "device uuid",
            }),
        "node" => acp_core::model::NodeId::new(&id)
            .map(|access_node| AttachmentSubject::Node { access_node })
            .map_err(|_| StorageError::ColumnValue {
                column: "owned_attachment_transfer.subject_id",
                expected: "node uuid",
            }),
        _ => Err(StorageError::ColumnValue {
            column: "owned_attachment_transfer.subject_kind",
            expected: "device or node",
        }),
    }
}

/// 附件文件的最终路径：内容寻址文件名 = 43 字符规范 base64url 摘要（天然不含路径分隔符）。
fn final_path(config: &StorageConfig, digest: &Digest) -> PathBuf {
    config.attachment_dir.join(digest.as_str())
}

fn final_relative_path(digest: &Digest) -> String {
    format!("{}/{}", migrate::ATTACHMENTS_DIR, digest.as_str())
}

/// 传输临时文件的子目录：**不在** `sweep_orphans` 扫描的顶层（它只看顶层 `is_file()` 条目），
/// 因此 60 秒一次、以 `now` 为界的孤儿回收不会删掉正在上传的块文件。
const TRANSFER_DIR: &str = ".transfers";

fn temporary_dir(config: &StorageConfig) -> PathBuf {
    config.attachment_dir.join(TRANSFER_DIR)
}

/// 临时文件路径：随机 UUID（`begin_transfer` 生成）不可预测，且落在专用子目录里。
fn temporary_path(config: &StorageConfig, transfer: &AttachmentId) -> PathBuf {
    temporary_dir(config).join(format!("{}.part", transfer.as_str()))
}

/// Completes the filesystem half of session deletion.
///
/// The deletion transaction commits the tombstone before calling this function,
/// so a crash cannot leave a live session with missing transfer bytes. Transfer
/// rows remain as the durable locator until their `.part` file is gone; only
/// then are the rows removed. Cleanup is best effort after the authoritative
/// deletion commit: failures retain the locator, emit a structured warning, and
/// are retried by the next delete, startup, or pruning pass without changing the
/// already committed command result.
pub(crate) async fn cleanup_tombstoned_transfer_files(
    pool: &sqlx::SqlitePool,
    config: &StorageConfig,
    session: Option<&SessionId>,
) {
    match cleanup_tombstoned_transfer_files_inner(pool, config, session).await {
        Ok(failures) if !failures.is_empty() => {
            eprintln!("{}", transfer_cleanup_warning(session, &failures));
        }
        Ok(_) => {}
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::json!({
                    "level": "warn",
                    "event": "storage.tombstoned_transfer_cleanup_failed",
                    "session_id": session.map(SessionId::as_str),
                    "error": error.to_string(),
                    "message": "failed to finish tombstoned transfer cleanup; durable locators remain for retry",
                })
            );
        }
    }
}

#[derive(Debug)]
struct TransferCleanupFailure {
    session_id: String,
    transfer_id: String,
    error: String,
}

fn transfer_cleanup_warning(
    session: Option<&SessionId>,
    failures: &[TransferCleanupFailure],
) -> serde_json::Value {
    serde_json::json!({
        "level": "warn",
        "event": "storage.tombstoned_transfer_file_cleanup_failed",
        "session_id": session.map(SessionId::as_str),
        "failed_transfer_count": failures.len(),
        "failures": failures.iter().map(|failure| {
            serde_json::json!({
                "session_id": failure.session_id,
                "transfer_id": failure.transfer_id,
                "error": failure.error,
            })
        }).collect::<Vec<_>>(),
        "message": "failed to remove tombstoned transfer files; durable locators retained",
    })
}

async fn cleanup_tombstoned_transfer_files_inner(
    pool: &sqlx::SqlitePool,
    config: &StorageConfig,
    session: Option<&SessionId>,
) -> Result<Vec<TransferCleanupFailure>, StorageError> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.db()?;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT transfer_id, session_id FROM owned_attachment_transfer t \
         WHERE (?1 IS NULL OR t.session_id = ?1) \
           AND EXISTS (SELECT 1 FROM owned_session s \
                       WHERE s.session_id = t.session_id AND s.deleted_at IS NOT NULL) \
         ORDER BY t.session_id, t.transfer_id",
    )
    .bind(session.map(SessionId::as_str))
    .fetch_all(&mut *tx)
    .await
    .db()?;

    let mut failures = Vec::new();
    for (transfer_text, session_text) in &rows {
        let transfer = AttachmentId::new(transfer_text).map_err(|_| StorageError::ColumnValue {
            column: "owned_attachment_transfer.transfer_id",
            expected: "canonical uuid",
        })?;
        match std::fs::remove_file(temporary_path(config, &transfer)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                failures.push(TransferCleanupFailure {
                    session_id: session_text.clone(),
                    transfer_id: transfer.as_str().to_owned(),
                    error: error.to_string(),
                });
                continue;
            }
        }
        sqlx::query("DELETE FROM owned_attachment_transfer WHERE transfer_id = ?1")
            .bind(transfer.as_str())
            .execute(&mut *tx)
            .await
            .db()?;
    }
    tx.commit().await.db()?;
    Ok(failures)
}

fn io_error() -> PortError {
    PortError::Unavailable(UnavailableKind::IoError)
}

/// 清单摘要的规范编码：按清单顺序串接每个附件标识的 `u32` 大端长度前缀与 UTF-8 字节，再取 SHA-256。
///
/// 该算法同时用于 `attachment.manifest_sealed` 的校验与服务端自算，`docs/CORE_PORTS_AND_STORAGE.md`
/// §5.8 是唯一权威描述；不依赖 JSON 序列化，因此同一清单在任何实现上得到同一摘要。
fn manifest_digest(attachments: &[AttachmentId]) -> Digest {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    for attachment in attachments {
        let bytes = attachment.as_str().as_bytes();
        hasher.update((bytes.len() as u32).to_be_bytes());
        hasher.update(bytes);
    }
    let encoded = {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
    };
    Digest::new(&encoded).expect("sha256 digest is canonical base64url")
}

/// 流式哈希一个已写入的文件（内存只占用一块）。
fn stream_digest(path: &Path) -> Result<Digest, PortError> {
    use sha2::Digest as _;
    let mut file = std::fs::File::open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => PortError::Corrupt("attachment bytes are missing"),
        _ => io_error(),
    })?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; CHUNK_SIZE];
    loop {
        let read = file.read(&mut buffer).map_err(|_| io_error())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let encoded = {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
    };
    Digest::new(&encoded).map_err(|_| PortError::Corrupt("computed digest is not canonical"))
}

fn transfer_from_row(
    row: &SqliteRow,
    sealed: Option<SealedAttachment>,
) -> Result<AttachmentTransfer, StorageError> {
    let state: AttachmentTransferState =
        text(row, "state")?
            .parse()
            .map_err(|_| StorageError::ColumnValue {
                column: "owned_attachment_transfer.state",
                expected: "receiving|sealed|aborted|expired",
            })?;
    Ok(AttachmentTransfer {
        transfer: AttachmentId::new(&text(row, "transfer_id")?).map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_attachment_transfer.transfer_id",
                expected: "canonical uuid",
            }
        })?,
        session: SessionId::new(&text(row, "session_id")?).map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_attachment_transfer.session_id",
                expected: "canonical uuid",
            }
        })?,
        subject: subject_from_row(row)?,
        total_length: u64::try_from(int(row, "total_length")?).map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_attachment_transfer.total_length",
                expected: "non-negative integer",
            }
        })?,
        received_length: u64::try_from(int(row, "received_length")?).map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_attachment_transfer.received_length",
                expected: "non-negative integer",
            }
        })?,
        chunk_size: u32::try_from(int(row, "chunk_size")?).map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_attachment_transfer.chunk_size",
                expected: "32768",
            }
        })?,
        state,
        sealed,
        created_at: Timestamp::new(&text(row, "created_at")?).map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_attachment_transfer.created_at",
                expected: "timestamp",
            }
        })?,
        updated_at: Timestamp::new(&text(row, "updated_at")?).map_err(|_| {
            StorageError::ColumnValue {
                column: "owned_attachment_transfer.updated_at",
                expected: "timestamp",
            }
        })?,
    })
}

const TRANSFER_COLUMNS: &str = "transfer_id, session_id, subject_kind, subject_id, total_length, \
     received_length, chunk_size, state, media_type, file_name, sealed_sha256, created_at, updated_at";

async fn load_transfer_row(
    ex: &mut sqlx::SqliteConnection,
    transfer: &AttachmentId,
) -> Result<Option<SqliteRow>, PortError> {
    Ok(sqlx::query(&format!(
        "SELECT {TRANSFER_COLUMNS} FROM owned_attachment_transfer t \
         WHERE t.transfer_id = ?1 \
           AND EXISTS (SELECT 1 FROM owned_session s \
                       WHERE s.session_id = t.session_id AND {})",
        crate::session_store::visible_session(Some("s"))
    ))
    .bind(transfer.as_str())
    .fetch_optional(&mut *ex)
    .await
    .db()?)
}

/// 由内容摘要取实际生效的附件行 id（内容寻址去重后可能与本次传输的 id 不同）。
async fn attachment_id_for_digest(
    ex: &mut sqlx::SqliteConnection,
    digest: &Digest,
) -> Result<Option<AttachmentId>, PortError> {
    let id: Option<String> =
        sqlx::query_scalar("SELECT attachment_id FROM owned_attachment WHERE sha256 = ?1")
            .bind(digest.as_str())
            .fetch_optional(&mut *ex)
            .await
            .db()?;
    id.map(|text| {
        AttachmentId::new(&text)
            .map_err(|_| PortError::Corrupt("owned_attachment.attachment_id is malformed"))
    })
    .transpose()
}

async fn sealed_of(
    ex: &mut sqlx::SqliteConnection,
    row: &SqliteRow,
    at: &Timestamp,
) -> Result<Option<SealedAttachment>, PortError> {
    let Some(sha) = opt_text(row, "sealed_sha256")? else {
        return Ok(None);
    };
    let sha256 = Digest::new(&sha).map_err(|_| PortError::Corrupt("sealed_sha256 is malformed"))?;
    let Some(attachment) = attachment_id_for_digest(ex, &sha256).await? else {
        return Ok(None);
    };
    let length = u64::try_from(int(row, "total_length")?)
        .map_err(|_| PortError::Corrupt("total_length is negative"))?;
    let sealed_at =
        Timestamp::new(at.as_str()).map_err(|_| PortError::InvalidRequest("timestamp shape"))?;
    Ok(Some(SealedAttachment {
        attachment,
        length,
        sha256,
        sealed_at,
    }))
}

async fn submission_attachments(
    ex: &mut sqlx::SqliteConnection,
    submission: &SubmissionId,
) -> Result<Vec<AttachmentId>, PortError> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT attachment_id FROM owned_attachment_submission_item \
         WHERE submission_id = ?1 ORDER BY position ASC",
    )
    .bind(submission.as_str())
    .fetch_all(&mut *ex)
    .await
    .db()?;
    rows.into_iter()
        .map(|text| {
            AttachmentId::new(&text)
                .map_err(|_| PortError::Corrupt("submission item is not a canonical uuid"))
        })
        .collect()
}

fn submission_row_to_model(
    row: &SqliteRow,
    attachments: Vec<AttachmentId>,
) -> Result<AttachmentSubmission, PortError> {
    let state = text(row, "state").map_err(|_| PortError::Corrupt("submission state"))?;
    let digest_text =
        opt_text(row, "manifest_digest").map_err(|_| PortError::Corrupt("submission digest"))?;
    let sealed_at = opt_text(row, "sealed_at").map_err(|_| PortError::Corrupt("sealed_at"))?;
    let (Some(digest_text), Some(sealed_at)) = (digest_text, sealed_at) else {
        return Err(PortError::InvalidRequest("submission is not sealed"));
    };
    let _ = state;
    Ok(AttachmentSubmission {
        submission: SubmissionId::new(
            &text(row, "submission_id")
                .map_err(|_| PortError::Corrupt("submission id is malformed"))?,
        )
        .map_err(|_| PortError::Corrupt("submission id is malformed"))?,
        session: SessionId::new(
            &text(row, "session_id").map_err(|_| PortError::Corrupt("submission session"))?,
        )
        .map_err(|_| PortError::Corrupt("submission session is malformed"))?,
        subject: subject_from_row(row).map_err(|_| PortError::Corrupt("submission subject"))?,
        attachments,
        digest: Digest::new(&digest_text)
            .map_err(|_| PortError::Corrupt("submission digest is malformed"))?,
        sealed_at: Timestamp::new(&sealed_at)
            .map_err(|_| PortError::Corrupt("submission sealed_at is malformed"))?,
    })
}

async fn progress_of(
    ex: &mut sqlx::SqliteConnection,
    submission: &SubmissionId,
) -> Result<SubmissionProgress, PortError> {
    let row = sqlx::query(
        "SELECT submission_id, state, attachment_count FROM owned_attachment_submission \
         WHERE submission_id = ?1",
    )
    .bind(submission.as_str())
    .fetch_optional(&mut *ex)
    .await
    .db()?
    .ok_or_else(|| PortError::NotFound(EntityRef::Submission(submission.clone())))?;
    let registered: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM owned_attachment_submission_item WHERE submission_id = ?1",
    )
    .bind(submission.as_str())
    .fetch_one(&mut *ex)
    .await
    .db()?;
    Ok(SubmissionProgress {
        submission: submission.clone(),
        registered: u64::try_from(registered).unwrap_or(0),
        attachment_count: u64::try_from(int(&row, "attachment_count")?)
            .map_err(|_| PortError::Corrupt("submission count is negative"))?,
        sealed: text(&row, "state").map_err(|_| PortError::Corrupt("submission state"))?
            == "sealed",
    })
}

#[async_trait]
impl AttachmentTransferStore for SqliteStore {
    async fn begin_transfer(&self, write: TransferBegin) -> Result<AttachmentTransfer, PortError> {
        self.writable()?;
        if write.media_type.is_empty() {
            return Err(PortError::InvalidRequest("attachment media type is empty"));
        }
        if write.file_name.is_empty() {
            return Err(PortError::InvalidRequest("attachment file name is empty"));
        }
        if write.total_length > self.config().attachment_max_file_bytes {
            return Err(PortError::InvalidRequest(
                "attachment exceeds storage.attachment_max_file_bytes",
            ));
        }
        let (subject_kind, subject_id) = subject_columns(&write.subject);
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let exists: Option<i64> = sqlx::query_scalar(&format!(
            "SELECT 1 FROM owned_session WHERE session_id = ?1 AND {}",
            crate::session_store::visible_session(None)
        ))
        .bind(write.session.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?;
        if exists.is_none() {
            return Err(PortError::NotFound(EntityRef::Session(write.session)));
        }
        let prior = load_transfer_row(&mut tx, &write.transfer).await?;
        if let Some(row) = &prior {
            let same = text(row, "session_id")? == write.session.as_str()
                && text(row, "subject_kind")? == subject_kind
                && text(row, "subject_id")? == subject_id
                && u64::try_from(int(row, "total_length")?).ok() == Some(write.total_length)
                && text(row, "media_type")? == write.media_type
                && text(row, "file_name")? == write.file_name;
            if !same {
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            // 幂等重发：不重置已接收长度（重复 begin 不得让已确认的块失效）。
            let sealed = sealed_of(&mut tx, row, &write.at).await?;
            let transfer = transfer_from_row(row, sealed).map_err(PortError::from)?;
            tx.commit().await.db()?;
            return Ok(transfer);
        }
        // 临时文件先建出来：磁盘不可用时立即失败，不留下行。
        std::fs::create_dir_all(temporary_dir(self.config())).map_err(|_| io_error())?;
        let temporary = temporary_path(self.config(), &write.transfer);
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|_| io_error())?;
        migrate::tighten_file_permissions(&temporary).map_err(|_| io_error())?;
        sqlx::query(
            "INSERT INTO owned_attachment_transfer (transfer_id, session_id, subject_kind, \
             subject_id, total_length, received_length, chunk_size, state, media_type, file_name, \
             sealed_sha256, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, 'receiving', ?7, ?8, NULL, ?9, ?9)",
        )
        .bind(write.transfer.as_str())
        .bind(write.session.as_str())
        .bind(subject_kind)
        .bind(subject_id)
        .bind(write.total_length as i64)
        .bind(CHUNK_SIZE as i64)
        .bind(&write.media_type)
        .bind(&write.file_name)
        .bind(write.at.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        tx.commit().await.db()?;
        let mut read = self.acquire_read().await?;
        let row = load_transfer_row(&mut read, &write.transfer)
            .await?
            .ok_or(PortError::Corrupt("transfer row vanished after commit"))?;
        transfer_from_row(&row, None).map_err(PortError::from)
    }

    async fn append_chunk(&self, write: TransferChunk) -> Result<ChunkOutcome, PortError> {
        self.writable()?;
        if write.data.is_empty() || write.data.len() > CHUNK_SIZE {
            return Err(PortError::InvalidRequest(
                "chunk size must be 1..=32768 bytes",
            ));
        }
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let row = load_transfer_row(&mut tx, &write.transfer)
            .await?
            .ok_or_else(|| PortError::NotFound(EntityRef::Attachment(write.transfer.clone())))?;
        let state: AttachmentTransferState = text(&row, "state")?.parse().map_err(|_| {
            PortError::Corrupt("owned_attachment_transfer.state is not a known state")
        })?;
        if state != AttachmentTransferState::Receiving {
            return Err(PortError::InvalidRequest("transfer is not receiving"));
        }
        let received = u64::try_from(int(&row, "received_length")?)
            .map_err(|_| PortError::Corrupt("received_length is negative"))?;
        let total = u64::try_from(int(&row, "total_length")?)
            .map_err(|_| PortError::Corrupt("total_length is negative"))?;
        let path = temporary_path(self.config(), &write.transfer);
        let length = write.data.len() as u64;
        if write.offset == received {
            let next = received
                .checked_add(length)
                .filter(|next| *next <= total)
                .ok_or(PortError::InvalidRequest(
                    "chunk would exceed the declared total length",
                ))?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|error| match error.kind() {
                    std::io::ErrorKind::NotFound => {
                        PortError::Corrupt("transfer payload file is missing")
                    }
                    _ => io_error(),
                })?;
            file.seek(SeekFrom::Start(received))
                .map_err(|_| io_error())?;
            file.write_all(&write.data).map_err(|_| io_error())?;
            file.flush().map_err(|_| io_error())?;
            let affected = sqlx::query(
                "UPDATE owned_attachment_transfer SET received_length = ?1, updated_at = ?2 \
                 WHERE transfer_id = ?3 AND state = 'receiving' AND received_length = ?4",
            )
            .bind(storable(next)?)
            .bind(write.at.as_str())
            .bind(write.transfer.as_str())
            .bind(storable(received)?)
            .execute(&mut *tx)
            .await
            .db()?
            .rows_affected();
            if affected != 1 {
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            tx.commit().await.db()?;
            return Ok(ChunkOutcome {
                next_offset: next,
                duplicate: false,
            });
        }
        if write.offset > received {
            return Err(PortError::InvalidRequest(
                "chunk offset does not match the next expected offset",
            ));
        }
        // 已确认区间内的重复块：内容相同返回原确认，不同内容拒绝（§5.8）。
        let available = received - write.offset;
        if available < length {
            return Err(PortError::InvalidRequest(
                "chunk offset does not match the next expected offset",
            ));
        }
        let mut file = std::fs::File::open(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => PortError::Corrupt("transfer payload file is missing"),
            _ => io_error(),
        })?;
        file.seek(SeekFrom::Start(write.offset))
            .map_err(|_| io_error())?;
        let mut existing = vec![0_u8; write.data.len()];
        file.read_exact(&mut existing).map_err(|_| io_error())?;
        if existing != write.data {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        tx.commit().await.db()?;
        Ok(ChunkOutcome {
            next_offset: received,
            duplicate: true,
        })
    }

    async fn seal_transfer(&self, write: TransferSeal) -> Result<SealedAttachment, PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let row = load_transfer_row(&mut tx, &write.transfer)
            .await?
            .ok_or_else(|| PortError::NotFound(EntityRef::Attachment(write.transfer.clone())))?;
        let state: AttachmentTransferState = text(&row, "state")?.parse().map_err(|_| {
            PortError::Corrupt("owned_attachment_transfer.state is not a known state")
        })?;
        if state == AttachmentTransferState::Sealed {
            let sealed_sha = opt_text(&row, "sealed_sha256")?.unwrap_or_default();
            let sealed_length = u64::try_from(int(&row, "total_length")?).unwrap_or(u64::MAX);
            if sealed_sha == write.sha256.as_str() && sealed_length == write.length {
                let sealed = sealed_of(&mut tx, &row, &write.at)
                    .await?
                    .ok_or(PortError::Corrupt("sealed transfer has no attachment row"))?;
                tx.commit().await.db()?;
                return Ok(sealed);
            }
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        if state != AttachmentTransferState::Receiving {
            return Err(PortError::InvalidRequest("transfer is not receiving"));
        }
        let received = u64::try_from(int(&row, "received_length")?)
            .map_err(|_| PortError::Corrupt("received_length is negative"))?;
        let total = u64::try_from(int(&row, "total_length")?)
            .map_err(|_| PortError::Corrupt("total_length is negative"))?;
        if write.length != total || received != total {
            return Err(PortError::InvalidRequest(
                "sealed length does not match the received bytes",
            ));
        }
        let temporary = temporary_path(self.config(), &write.transfer);
        let final_file = final_path(self.config(), &write.sha256);
        // 崩溃窗口（rename 已发生、事务未提交）后重试：临时文件不在而最终文件在，视为已完成 rename。
        if !temporary.exists() {
            if !final_file.exists() {
                return Err(PortError::Corrupt("transfer payload file is missing"));
            }
        } else {
            let actual = stream_digest(&temporary)?;
            if actual != write.sha256 {
                return Err(PortError::InvalidRequest(
                    "attachment digest does not match the streamed bytes",
                ));
            }
            if final_file.exists() {
                // 内容寻址：同样的字节只存一份。
                std::fs::remove_file(&temporary).map_err(|_| io_error())?;
            } else {
                std::fs::rename(&temporary, &final_file).map_err(|_| io_error())?;
                migrate::tighten_file_permissions(&final_file).map_err(|_| io_error())?;
            }
        }
        sqlx::query(
            "INSERT INTO owned_attachment (attachment_id, sha256, byte_length, media_type, \
             relative_path, created_at, last_used_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6) \
             ON CONFLICT(sha256) DO UPDATE SET last_used_at = excluded.last_used_at",
        )
        .bind(write.transfer.as_str())
        .bind(write.sha256.as_str())
        .bind(storable(total)?)
        .bind(text(&row, "media_type")?)
        .bind(final_relative_path(&write.sha256))
        .bind(write.at.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        let attachment = attachment_id_for_digest(&mut tx, &write.sha256)
            .await?
            .ok_or(PortError::Corrupt("sealed attachment row vanished"))?;
        let affected = sqlx::query(
            "UPDATE owned_attachment_transfer SET state = 'sealed', sealed_sha256 = ?1, \
             updated_at = ?2 WHERE transfer_id = ?3 AND state = 'receiving'",
        )
        .bind(write.sha256.as_str())
        .bind(write.at.as_str())
        .bind(write.transfer.as_str())
        .execute(&mut *tx)
        .await
        .db()?
        .rows_affected();
        if affected != 1 {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        tx.commit().await.db()?;
        Ok(SealedAttachment {
            attachment,
            length: total,
            sha256: write.sha256,
            sealed_at: write.at,
        })
    }

    async fn transfer(
        &self,
        transfer: &AttachmentId,
    ) -> Result<Option<AttachmentTransfer>, PortError> {
        let mut read = self.acquire_read().await?;
        let Some(row) = load_transfer_row(&mut read, transfer).await? else {
            return Ok(None);
        };
        let updated_at = Timestamp::new(&text(&row, "updated_at")?)
            .map_err(|_| PortError::Corrupt("transfer updated_at"))?;
        let sealed = sealed_of(&mut read, &row, &updated_at).await?;
        transfer_from_row(&row, sealed)
            .map(Some)
            .map_err(PortError::from)
    }

    async fn abort_transfer(
        &self,
        transfer: &AttachmentId,
        at: Timestamp,
    ) -> Result<AttachmentTransfer, PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let affected = sqlx::query(
            "UPDATE owned_attachment_transfer SET state = 'aborted', updated_at = ?1 \
             WHERE transfer_id = ?2 AND state = 'receiving'",
        )
        .bind(at.as_str())
        .bind(transfer.as_str())
        .execute(&mut *tx)
        .await
        .db()?
        .rows_affected();
        let row = load_transfer_row(&mut tx, transfer)
            .await?
            .ok_or_else(|| PortError::NotFound(EntityRef::Attachment(transfer.clone())))?;
        let state: AttachmentTransferState = text(&row, "state")?.parse().map_err(|_| {
            PortError::Corrupt("owned_attachment_transfer.state is not a known state")
        })?;
        if affected == 0 && state == AttachmentTransferState::Receiving {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        if state == AttachmentTransferState::Sealed {
            return Err(PortError::InvalidRequest(
                "a sealed transfer cannot be aborted",
            ));
        }
        tx.commit().await.db()?;
        if state == AttachmentTransferState::Aborted {
            let _ = std::fs::remove_file(temporary_path(self.config(), transfer));
        }
        transfer_from_row(&row, None).map_err(PortError::from)
    }

    async fn begin_submission(
        &self,
        write: SubmissionBegin,
    ) -> Result<SubmissionProgress, PortError> {
        self.writable()?;
        if write.attachment_count == 0 {
            return Err(PortError::InvalidRequest(
                "an attachment manifest must not be empty",
            ));
        }
        let (subject_kind, subject_id) = subject_columns(&write.subject);
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let exists: Option<i64> = sqlx::query_scalar(&format!(
            "SELECT 1 FROM owned_session WHERE session_id = ?1 AND {}",
            crate::session_store::visible_session(None)
        ))
        .bind(write.session.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?;
        if exists.is_none() {
            return Err(PortError::NotFound(EntityRef::Session(write.session)));
        }
        let prior = sqlx::query(
            "SELECT submission_id, session_id, subject_kind, subject_id, state, attachment_count \
             FROM owned_attachment_submission WHERE submission_id = ?1",
        )
        .bind(write.submission.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?;
        if let Some(row) = &prior {
            let same = text(row, "session_id")? == write.session.as_str()
                && text(row, "subject_kind")? == subject_kind
                && text(row, "subject_id")? == subject_id
                && u64::try_from(int(row, "attachment_count")?).ok()
                    == Some(write.attachment_count);
            if !same {
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            let progress = progress_of(&mut tx, &write.submission).await?;
            tx.commit().await.db()?;
            return Ok(progress);
        }
        sqlx::query(
            "INSERT INTO owned_attachment_submission (submission_id, session_id, subject_kind, \
             subject_id, state, attachment_count, manifest_digest, created_at, sealed_at) \
             VALUES (?1, ?2, ?3, ?4, 'open', ?5, NULL, ?6, NULL)",
        )
        .bind(write.submission.as_str())
        .bind(write.session.as_str())
        .bind(subject_kind)
        .bind(subject_id)
        .bind(storable(write.attachment_count)?)
        .bind(write.at.as_str())
        .execute(&mut *tx)
        .await
        .db()?;
        let progress = progress_of(&mut tx, &write.submission).await?;
        tx.commit().await.db()?;
        Ok(progress)
    }

    async fn append_submission_page(
        &self,
        write: SubmissionPage,
    ) -> Result<SubmissionProgress, PortError> {
        self.writable()?;
        if write.attachments.is_empty() || write.attachments.len() > 64 {
            return Err(PortError::InvalidRequest(
                "a manifest page carries 1..=64 attachments",
            ));
        }
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let row = sqlx::query(&format!(
            "SELECT state, attachment_count FROM owned_attachment_submission \
             WHERE submission_id = ?1 \
               AND EXISTS (SELECT 1 FROM owned_session s \
                           WHERE s.session_id = owned_attachment_submission.session_id \
                             AND {})",
            crate::session_store::visible_session(Some("s"))
        ))
        .bind(write.submission.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?
        .ok_or_else(|| PortError::NotFound(EntityRef::Submission(write.submission.clone())))?;
        if text(&row, "state")? != "open" {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        let declared = u64::try_from(int(&row, "attachment_count")?)
            .map_err(|_| PortError::Corrupt("attachment_count is negative"))?;
        let end = write.start_position + write.attachments.len() as u64;
        if end > declared {
            return Err(PortError::InvalidRequest(
                "manifest page exceeds the declared attachment count",
            ));
        }
        for (index, attachment) in write.attachments.iter().enumerate() {
            let position = write.start_position + index as u64;
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT attachment_id FROM owned_attachment_submission_item \
                 WHERE submission_id = ?1 AND position = ?2",
            )
            .bind(write.submission.as_str())
            .bind(storable(position)?)
            .fetch_optional(&mut *tx)
            .await
            .db()?;
            match existing {
                Some(found) if found == attachment.as_str() => continue,
                Some(_) => return Err(PortError::Conflict(ConflictKind::AlreadyExists)),
                None => {
                    sqlx::query(
                        "INSERT INTO owned_attachment_submission_item \
                         (submission_id, position, attachment_id) VALUES (?1, ?2, ?3)",
                    )
                    .bind(write.submission.as_str())
                    .bind(storable(position)?)
                    .bind(attachment.as_str())
                    .execute(&mut *tx)
                    .await
                    .db()?;
                }
            }
        }
        let progress = progress_of(&mut tx, &write.submission).await?;
        tx.commit().await.db()?;
        Ok(progress)
    }

    async fn finalize_submission(
        &self,
        write: SubmissionSeal,
    ) -> Result<AttachmentSubmission, PortError> {
        self.writable()?;
        let (subject_kind, subject_id) = subject_columns(&write.subject);
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let row = sqlx::query(&format!(
            "SELECT submission_id, session_id, subject_kind, subject_id, state, attachment_count, \
             manifest_digest, sealed_at FROM owned_attachment_submission WHERE submission_id = ?1 \
               AND EXISTS (SELECT 1 FROM owned_session s \
                           WHERE s.session_id = owned_attachment_submission.session_id \
                             AND {})",
            crate::session_store::visible_session(Some("s"))
        ))
        .bind(write.submission.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db()?
        .ok_or_else(|| PortError::NotFound(EntityRef::Submission(write.submission.clone())))?;
        let state = text(&row, "state")?;
        if state == "sealed" {
            let existing = submission_attachments(&mut tx, &write.submission).await?;
            if existing == write.attachments
                && opt_text(&row, "manifest_digest")?.as_deref() == Some(write.digest.as_str())
            {
                let model = submission_row_to_model(&row, existing)?;
                tx.commit().await.db()?;
                return Ok(model);
            }
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        if text(&row, "session_id")? != write.session.as_str()
            || text(&row, "subject_kind")? != subject_kind
            || text(&row, "subject_id")? != subject_id
        {
            return Err(PortError::InvalidRequest(
                "attachment manifest belongs to another session or subject",
            ));
        }
        let declared = u64::try_from(int(&row, "attachment_count")?)
            .map_err(|_| PortError::Corrupt("attachment_count is negative"))?;
        if declared != write.attachments.len() as u64 {
            return Err(PortError::InvalidRequest(
                "manifest count does not match the sealed attachment list",
            ));
        }
        let registered = submission_attachments(&mut tx, &write.submission).await?;
        if registered != write.attachments {
            return Err(PortError::InvalidRequest(
                "manifest pages do not match the sealed attachment list",
            ));
        }
        let computed = manifest_digest(&write.attachments);
        if computed != write.digest {
            return Err(PortError::InvalidRequest(
                "manifest digest does not match the registered list",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for attachment in &write.attachments {
            if !seen.insert(attachment.as_str()) {
                return Err(PortError::InvalidRequest(
                    "manifest contains the same attachment twice",
                ));
            }
            let path: Option<String> = sqlx::query_scalar(
                "SELECT relative_path FROM owned_attachment WHERE attachment_id = ?1",
            )
            .bind(attachment.as_str())
            .fetch_optional(&mut *tx)
            .await
            .db()?;
            let Some(relative) = path else {
                return Err(PortError::InvalidRequest(
                    "manifest references an attachment that is not sealed",
                ));
            };
            let file = crate::session_store::attachment_file_path(self.config(), &relative);
            if !file.is_file() {
                return Err(PortError::Corrupt("attachment bytes are missing"));
            }
        }
        let affected = sqlx::query(
            "UPDATE owned_attachment_submission SET state = 'sealed', manifest_digest = ?1, \
             sealed_at = ?2 WHERE submission_id = ?3 AND state = 'open'",
        )
        .bind(write.digest.as_str())
        .bind(write.at.as_str())
        .bind(write.submission.as_str())
        .execute(&mut *tx)
        .await
        .db()?
        .rows_affected();
        if affected != 1 {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        let sealed = sqlx::query(
            "SELECT submission_id, session_id, subject_kind, subject_id, state, attachment_count, \
             manifest_digest, sealed_at FROM owned_attachment_submission WHERE submission_id = ?1",
        )
        .bind(write.submission.as_str())
        .fetch_one(&mut *tx)
        .await
        .db()?;
        let model = submission_row_to_model(&sealed, write.attachments)?;
        tx.commit().await.db()?;
        Ok(model)
    }

    async fn submission(
        &self,
        submission: &SubmissionId,
    ) -> Result<Option<AttachmentSubmission>, PortError> {
        let mut read = self.acquire_read().await?;
        let row = sqlx::query(&format!(
            "SELECT submission_id, session_id, subject_kind, subject_id, state, attachment_count, \
             manifest_digest, sealed_at FROM owned_attachment_submission \
             WHERE submission_id = ?1 \
               AND EXISTS (SELECT 1 FROM owned_session s \
                           WHERE s.session_id = owned_attachment_submission.session_id \
                             AND {})",
            crate::session_store::visible_session(Some("s"))
        ))
        .bind(submission.as_str())
        .fetch_optional(&mut *read)
        .await
        .db()?;
        let Some(row) = row else {
            return Ok(None);
        };
        if text(&row, "state")? != "sealed" {
            // 未封存清单不可被 prompt 引用；调用方按「没有可用清单」处理，不派发 Agent。
            return Ok(None);
        }
        let attachments = submission_attachments(&mut read, submission).await?;
        submission_row_to_model(&row, attachments).map(Some)
    }

    async fn read_chunk(&self, request: &AttachmentRead) -> Result<AttachmentChunk, PortError> {
        let limit = request.max_length.min(CHUNK_SIZE as u64) as usize;
        if limit == 0 {
            return Err(PortError::InvalidRequest("max_length must be positive"));
        }
        let mut read = self.acquire_read().await?;
        let row = sqlx::query(&format!(
            "SELECT attachment_id, byte_length, relative_path FROM owned_attachment \
             WHERE attachment_id = ?1 \
               AND EXISTS (SELECT 1 FROM owned_session s WHERE s.session_id = ?2 AND {})",
            crate::session_store::visible_session(Some("s"))
        ))
        .bind(request.attachment.as_str())
        .bind(request.session.as_str())
        .fetch_optional(&mut *read)
        .await
        .db()?
        .ok_or_else(|| PortError::NotFound(EntityRef::Attachment(request.attachment.clone())))?;
        let total = u64::try_from(int(&row, "byte_length")?)
            .map_err(|_| PortError::Corrupt("byte_length is negative"))?;
        // 归属校验：附件必须已绑定到该会话（`link`）或作为该会话消息的媒体引用存在。
        let bound: Option<i64> = sqlx::query_scalar(
            "SELECT 1 WHERE \
               EXISTS (SELECT 1 FROM owned_attachment_link \
                       WHERE session_id = ?1 AND attachment_id = ?2) \
               OR EXISTS (SELECT 1 FROM owned_media_ref r \
                          JOIN owned_message m ON m.message_id = r.message_id \
                          WHERE m.session_id = ?1 AND r.attachment_id = ?2)",
        )
        .bind(request.session.as_str())
        .bind(request.attachment.as_str())
        .fetch_optional(&mut *read)
        .await
        .db()?;
        if bound.is_none() {
            return Err(PortError::NotFound(EntityRef::Attachment(
                request.attachment.clone(),
            )));
        }
        if request.offset > total {
            return Err(PortError::InvalidRequest("read offset is past the end"));
        }
        let relative = text(&row, "relative_path")?;
        let path = crate::session_store::attachment_file_path(self.config(), &relative);
        let mut file = std::fs::File::open(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => PortError::Corrupt("attachment bytes are missing"),
            _ => io_error(),
        })?;
        file.seek(SeekFrom::Start(request.offset))
            .map_err(|_| io_error())?;
        let want = usize::try_from((total - request.offset).min(limit as u64)).unwrap_or(limit);
        let mut data = vec![0_u8; want];
        file.read_exact(&mut data).map_err(|_| io_error())?;
        Ok(AttachmentChunk {
            offset: request.offset,
            eof: request.offset + data.len() as u64 >= total,
            data,
            total_length: total,
        })
    }

    async fn prune_transfers(&self, at: Timestamp, limit: u32) -> Result<u32, PortError> {
        self.writable()?;
        cleanup_tombstoned_transfer_files(&self.pools().write, self.config(), None).await;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let (expired, _, files) = expire_stale_uploads(&mut tx, self.config(), &at, limit).await?;
        tx.commit().await.db()?;
        // 文件删除在事务提交之后（§6 第 18 条）：即使文件删除失败，行已经是 `expired`，下次清理会
        // 通过孤儿回收删除残留文件。
        for path in files {
            let _ = std::fs::remove_file(path);
        }
        Ok(expired)
    }
}

/// 闲置清理的共享实现（`prune_transfers` 与 `SessionStore::prune` 共用）。
///
/// 顺序：① 过期未封存/已封存但闲置超过 24 小时的清单（条目经外键级联删除，从而释放它们对附件与
/// 传输的引用）；② 过期仍在接收且不再被活跃清单引用的传输行。文件删除由调用方在事务提交后执行。
///
/// 返回值：`(过期传输数, 过期清单数, 待删除的临时文件路径)`。
pub(crate) async fn expire_stale_uploads(
    ex: &mut sqlx::SqliteConnection,
    config: &StorageConfig,
    at: &Timestamp,
    limit: u32,
) -> Result<(u32, u32, Vec<PathBuf>), PortError> {
    if limit == 0 {
        return Ok((0, 0, Vec::new()));
    }
    let cutoff = stale_threshold(at)?;
    // ① 清单的生命周期：`open` 超过 24 小时、或 `sealed` 超过 24 小时仍未绑定（成功绑定会删除
    // 清单，见 `bind_submission`）都必须回收，否则 `submission_item` 会永久把附件钉成 GC 根。
    let manifests = sqlx::query(
        "DELETE FROM owned_attachment_submission \
         WHERE (state = 'open' AND created_at <= ?1) \
            OR (state = 'sealed' AND sealed_at IS NOT NULL AND sealed_at <= ?1)",
    )
    .bind(cutoff.as_str())
    .execute(&mut *ex)
    .await
    .db()?
    .rows_affected();
    let stale: Vec<String> = sqlx::query_scalar(
        "SELECT transfer_id FROM owned_attachment_transfer t \
         WHERE t.state = 'receiving' AND t.updated_at <= ?1 \
           AND NOT EXISTS (SELECT 1 FROM owned_attachment_submission_item i \
                           WHERE i.attachment_id = t.transfer_id) \
         ORDER BY t.updated_at ASC, t.transfer_id ASC LIMIT ?2",
    )
    .bind(cutoff.as_str())
    .bind(i64::from(limit))
    .fetch_all(&mut *ex)
    .await
    .db()?;
    let mut expired = 0_u32;
    let mut files = Vec::with_capacity(stale.len());
    for transfer in &stale {
        let affected = sqlx::query(
            "UPDATE owned_attachment_transfer SET state = 'expired', updated_at = ?1 \
             WHERE transfer_id = ?2 AND state = 'receiving'",
        )
        .bind(at.as_str())
        .bind(transfer)
        .execute(&mut *ex)
        .await
        .db()?
        .rows_affected();
        expired += affected as u32;
        if let Ok(id) = AttachmentId::new(transfer) {
            files.push(temporary_path(config, &id));
        }
    }
    Ok((expired, u32::try_from(manifests).unwrap_or(u32::MAX), files))
}

/// 已封存传输的附件行被 GC 后，把对应传输置为 `expired`（`sealed_sha256` 与 `state = sealed` 必须
/// 同有同无，因此两列一起改），避免出现「传输显示 sealed 但读不出附件」的状态机不一致。
pub(crate) async fn expire_transfers_for_digests(
    ex: &mut sqlx::SqliteConnection,
    at: &Timestamp,
    digests: &[String],
) -> Result<(), StorageError> {
    for digest in digests {
        sqlx::query(
            "UPDATE owned_attachment_transfer SET state = 'expired', sealed_sha256 = NULL, \
             updated_at = ?1 WHERE sealed_sha256 = ?2",
        )
        .bind(at.as_str())
        .bind(digest)
        .execute(&mut *ex)
        .await
        .db()?;
    }
    Ok(())
}

impl SqliteStore {
    /// 事务外读取用的连接（只读池）。
    pub(crate) async fn acquire_read(
        &self,
    ) -> Result<sqlx::pool::PoolConnection<sqlx::Sqlite>, PortError> {
        self.pools()
            .read
            .acquire()
            .await
            .map_err(|_| PortError::Unavailable(UnavailableKind::IoError))
    }
}

// ---------------------------------------------------------------------------------------------
// InteractionAuthority：有效待处理交互
// ---------------------------------------------------------------------------------------------

/// 从创建交互的事件 payload 里还原候选项与表单 schema（事件正文是权威，行里不存 options）。
fn options_from_view(view: &str) -> Result<Vec<InteractionOption>, PortError> {
    let value: serde_json::Value = serde_json::from_str(view)
        .map_err(|_| PortError::Corrupt("stored interaction payload is not json"))?;
    let Some(items) = value.get("options") else {
        return Ok(Vec::new());
    };
    let items = items
        .as_array()
        .ok_or(PortError::Corrupt("interaction options are not an array"))?;
    let mut options = Vec::with_capacity(items.len());
    for item in items {
        let option = item
            .as_object()
            .ok_or(PortError::Corrupt("interaction option is not an object"))?;
        let text = |key: &str| option.get(key).and_then(|value| value.as_str());
        let (Some(id), Some(label), Some(kind)) = (text("optionId"), text("label"), text("kind"))
        else {
            return Err(PortError::Corrupt(
                "interaction option misses a required field",
            ));
        };
        options.push(
            InteractionOption::try_new(id, label, kind)
                .map_err(|_| PortError::Corrupt("interaction option is invalid"))?,
        );
    }
    Ok(options)
}

fn schema_from_view(view: &str) -> Result<Option<ElicitationSchema>, PortError> {
    let value: serde_json::Value = serde_json::from_str(view)
        .map_err(|_| PortError::Corrupt("stored interaction payload is not json"))?;
    match value.get("schema") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Object(_)) => ElicitationSchema::new(
            &serde_json::to_string(value.get("schema").expect("schema is present"))
                .map_err(|_| PortError::Corrupt("stored schema is not serializable"))?,
        )
        .map(Some)
        .map_err(|_| PortError::Corrupt("stored elicitation schema is invalid")),
        Some(_) => Err(PortError::Corrupt(
            "stored elicitation schema is not an object",
        )),
    }
}

#[async_trait]
impl InteractionAuthority for SqliteStore {
    async fn effective(&self, session: &SessionId) -> Result<Vec<EffectiveInteraction>, PortError> {
        let mut read = self.acquire_read().await?;
        let rows = sqlx::query(
            "SELECT i.interaction_id, i.session_id, i.kind, i.request_version, i.created_at, \
                    e.payload_json, CAST(e.session_sequence AS TEXT) AS first_sequence, CAST(e.origin_sequence AS TEXT) AS first_origin_sequence \
             FROM owned_interaction i \
             LEFT JOIN owned_event e ON e.global_sequence = i.request_event \
             WHERE i.session_id = ?1 AND i.state = 'pending' AND i.invalidated_at IS NULL \
             AND EXISTS (SELECT 1 FROM owned_session s WHERE s.session_id=i.session_id \
                         AND s.deleted_at IS NULL) \
             ORDER BY i.created_at ASC, i.interaction_id ASC",
        )
        .bind(session.as_str())
        .fetch_all(&mut *read)
        .await
        .db()?;
        let mut effective = Vec::with_capacity(rows.len());
        for row in &rows {
            let kind: InteractionKind =
                text(row, "kind")?
                    .parse()
                    .map_err(|_| StorageError::ColumnValue {
                        column: "owned_interaction.kind",
                        expected: "permission|elicitation",
                    })?;
            let version = u64::try_from(int(row, "request_version")?)
                .map_err(|_| PortError::Corrupt("request_version is negative"))?;
            let view = text(row, "payload_json")
                .map_err(|_| PortError::Corrupt("interaction has no request event payload"))?;
            let (options, schema) = match kind {
                InteractionKind::Permission => (options_from_view(&view)?, None),
                InteractionKind::Elicitation => (Vec::new(), schema_from_view(&view)?),
            };
            let mut interaction = EffectiveInteraction::try_new(
                InteractionId::new(&text(row, "interaction_id")?).map_err(|_| {
                    StorageError::ColumnValue {
                        column: "owned_interaction.interaction_id",
                        expected: "canonical uuid",
                    }
                })?,
                SessionId::new(&text(row, "session_id")?).map_err(|_| {
                    StorageError::ColumnValue {
                        column: "owned_interaction.session_id",
                        expected: "canonical uuid",
                    }
                })?,
                kind,
                Version::new(version),
                None,
                options,
                schema,
            )
            .map_err(|_| PortError::Corrupt("stored interaction is invalid"))?;
            let position = |column| -> Result<Option<acp_core::model::Sequence>, PortError> {
                opt_text(row, column)?
                    .map(|value| {
                        value
                            .parse::<u64>()
                            .ok()
                            .and_then(|n| acp_core::model::Sequence::new(n).ok())
                            .ok_or(PortError::Corrupt("invalid interaction position"))
                    })
                    .transpose()
            };
            interaction.first_sequence = position("first_sequence")?;
            interaction.first_origin_sequence = position("first_origin_sequence")?;
            effective.push(interaction);
        }
        Ok(effective)
    }

    async fn invalidate_pending(
        &self,
        session: Option<&SessionId>,
        at: Timestamp,
    ) -> Result<u32, PortError> {
        self.writable()?;
        // 失效必须同时把 `state` 推进到 `expired`：所有「仍在等待」的读面（`effective`、`read_session`
        // 的 pending 集合、Node Link 快照）与旧的解析路径都按状态过滤，只写 `invalidated_at` 会让它们
        // 继续把失效请求当成可应答。失效范围是**仍未解决**的行：`pending` 与已仲裁但未确认的
        // `resolved`（`session-activity` D2：仲裁不等于处理完成）。已确认的行保留原状。
        // `invalidated_at` 保留失效时刻供诊断。
        let affected = match session {
            Some(session) => sqlx::query(
                "UPDATE owned_interaction SET state = 'expired', invalidated_at = ?1 \
                 WHERE invalidated_at IS NULL \
                 AND (state = 'pending' OR (state = 'resolved' AND confirmed_at IS NULL)) \
                 AND session_id = ?2",
            )
            .bind(at.as_str())
            .bind(session.as_str())
            .execute(&self.pools().write)
            .await
            .db()?,
            None => sqlx::query(
                "UPDATE owned_interaction SET state = 'expired', invalidated_at = ?1 \
                 WHERE invalidated_at IS NULL \
                 AND (state = 'pending' OR (state = 'resolved' AND confirmed_at IS NULL))",
            )
            .bind(at.as_str())
            .execute(&self.pools().write)
            .await
            .db()?,
        };
        Ok(u32::try_from(affected.rows_affected()).unwrap_or(u32::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_cleanup_warning_aggregates_all_failed_files() {
        let warning = transfer_cleanup_warning(
            Some(&SessionId::new("11111111-1111-4111-8111-111111111111").expect("session")),
            &[
                TransferCleanupFailure {
                    session_id: "11111111-1111-4111-8111-111111111111".to_owned(),
                    transfer_id: "22222222-2222-4222-8222-222222222222".to_owned(),
                    error: "first removal failed".to_owned(),
                },
                TransferCleanupFailure {
                    session_id: "11111111-1111-4111-8111-111111111111".to_owned(),
                    transfer_id: "33333333-3333-4333-8333-333333333333".to_owned(),
                    error: "second removal failed".to_owned(),
                },
            ],
        );

        assert_eq!(
            warning["event"],
            "storage.tombstoned_transfer_file_cleanup_failed"
        );
        assert_eq!(warning["failed_transfer_count"], 2);
        assert_eq!(
            warning["failures"]
                .as_array()
                .expect("aggregated failures")
                .len(),
            2
        );
    }
}
