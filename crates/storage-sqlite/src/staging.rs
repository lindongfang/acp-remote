//! 准备附件暂存（`docs/CORE_PORTS_AND_STORAGE.md` §5.11、`design.md` D6.4）。
//!
//! **没有数据库行**：传输/清单元数据只存在于内存，字节写在 Owner 私有内容区的专用子目录
//! （`<attachment_dir>/.preparation/`），与正式附件同一受管内容区。因此：
//!
//! - 准备阶段不需要（也不能）先建空会话绕过 `owned_session` 外键；
//! - 正常重启丢失运行期元数据，未受理的字节由 [`PreparationStaging::release_orphans`] 在启动/24 小时后
//!   回收；已受理的字节由首次提交事务重新挂到正式 `owned_attachment` 行上（rename，不复制）。
//!
//! 每次封存都用**流式** SHA-256（固定 32 KiB 缓冲），内存工作集与文件大小无关；封存后字节不再修改。

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use acp_core::model::{
    AttachmentId, AttachmentSubject, ClaimedAttachment, ConflictKind, ConnectionId, Digest,
    EntityRef, PortError, PreparationBinding, SealedFacts, StagedAbortReason, StagedChunk,
    StagedChunkOutcome, StagedClaim, StagedManifestBegin, StagedManifestPage, StagedManifestSeal,
    StagedManifestState, StagedSeal, StagedSealOutcome, StagedTransferBegin, StagedTransferState,
    SubmissionId, Timestamp, UnavailableKind,
};
use acp_core::ports::PreparationAttachmentStaging;

use crate::migrate::StorageConfig;

/// 与 wire/核心一致的固定单块上限。
const CHUNK_SIZE: usize = 32_768;
/// 与正式清单一致的每页上限。
const MANIFEST_PAGE: usize = 64;
/// 暂存子目录（不在 `sweep_orphans` 扫描的正式附件顶层里）。
const STAGING_DIR: &str = ".preparation";
const RECEIVING_DIR: &str = "receiving";
const SEALED_DIR: &str = "sealed";

#[derive(Debug, Clone)]
struct TransferRecord {
    binding: PreparationBinding,
    subject: AttachmentSubject,
    connection: ConnectionId,
    total_length: u64,
    received: u64,
    media_type: String,
    file_name: String,
    state: TransferState,
    sealed: Option<SealedFactsRecord>,
    updated_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferState {
    Receiving,
    Sealed,
}

#[derive(Debug, Clone)]
struct SealedFactsRecord {
    length: u64,
    sha256: Digest,
    sealed_at: Timestamp,
}

#[derive(Debug, Clone)]
struct ManifestRecord {
    binding: PreparationBinding,
    subject: AttachmentSubject,
    connection: ConnectionId,
    count: u32,
    transfers: Vec<AttachmentId>,
    sealed_digest: Option<Digest>,
    /// 首次提交正在认领：期间取消/清理不得移除它。
    claiming: bool,
    /// **本次**认领实际从封存路径 rename 到正式内容寻址路径的摘要。
    ///
    /// 正式路径上的字节也可能属于更早已提交的会话（[`PreparationStaging::begin_claim`] 接受该路径）；
    /// 失败回滚只能还原本清单自己搬走的字节，否则会破坏既有会话的媒体引用。
    renamed: Vec<Digest>,
    updated_at: Timestamp,
}

#[derive(Debug, Default)]
struct StagingState {
    transfers: HashMap<AttachmentId, TransferRecord>,
    manifests: HashMap<SubmissionId, ManifestRecord>,
}

/// 运行期准备暂存。所有状态在 [`Mutex`] 内；没有异步等待点持锁。
pub struct PreparationStaging {
    config: StorageConfig,
    state: Mutex<StagingState>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn io_error() -> PortError {
    PortError::Unavailable(UnavailableKind::IoError)
}

fn budget_error() -> PortError {
    PortError::Unavailable(UnavailableKind::StorageFull)
}

fn same_subject(left: &AttachmentSubject, right: &AttachmentSubject) -> bool {
    left == right
}

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

fn stream_digest(path: &Path) -> Result<Digest, PortError> {
    use sha2::Digest as _;
    let mut file = std::fs::File::open(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => PortError::Corrupt("staged attachment bytes are missing"),
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

fn system_time(ts: &Timestamp) -> SystemTime {
    let millis = ts.epoch_millis();
    if millis <= 0 {
        UNIX_EPOCH
    } else {
        UNIX_EPOCH + Duration::from_millis(millis as u64)
    }
}

fn older_than(path: &Path, before: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map(|modified| modified < before)
        .unwrap_or(false)
}

impl PreparationStaging {
    /// 构造（启动时尽力创建目录；失败推迟到第一次写入再报错）。
    pub fn new(config: StorageConfig) -> Self {
        let staging = Self {
            config,
            state: Mutex::new(StagingState::default()),
        };
        let _ = std::fs::create_dir_all(staging.receiving_dir());
        let _ = std::fs::create_dir_all(staging.sealed_dir());
        staging
    }

    fn staging_dir(&self) -> PathBuf {
        self.config.attachment_dir.join(STAGING_DIR)
    }

    fn receiving_dir(&self) -> PathBuf {
        self.staging_dir().join(RECEIVING_DIR)
    }

    fn sealed_dir(&self) -> PathBuf {
        self.staging_dir().join(SEALED_DIR)
    }

    fn receiving_path(&self, transfer: &AttachmentId) -> PathBuf {
        self.receiving_dir()
            .join(format!("{}.part", transfer.as_str()))
    }

    fn sealed_path(&self, digest: &Digest) -> PathBuf {
        self.sealed_dir().join(digest.as_str())
    }

    /// 正式附件内容的最终路径（与既有 `AttachmentStore` 同一目录）。
    pub(crate) fn owned_bytes_path(&self, digest: &Digest) -> PathBuf {
        self.config.attachment_dir.join(digest.as_str())
    }

    fn ensure_dirs(&self) -> Result<(), PortError> {
        std::fs::create_dir_all(self.receiving_dir()).map_err(|_| io_error())?;
        std::fs::create_dir_all(self.sealed_dir()).map_err(|_| io_error())
    }

    fn binding_matches(binding: &PreparationBinding, claim: &StagedClaim) -> bool {
        binding.device == claim.device
            && binding.session == claim.session
            && binding.runtime_epoch == claim.runtime_epoch
            && binding.generation == claim.generation
    }

    fn transfer_view(
        &self,
        transfer: &AttachmentId,
        record: &TransferRecord,
    ) -> StagedTransferState {
        StagedTransferState {
            transfer: transfer.clone(),
            binding: record.binding.clone(),
            received: record.received,
            total_length: record.total_length,
            sealed: record.sealed.as_ref().map(|sealed| SealedFacts {
                transfer: transfer.clone(),
                attachment: transfer.clone(),
                length: sealed.length,
                sha256: sealed.sha256.clone(),
                sealed_at: sealed.sealed_at.clone(),
            }),
            bound_connection: record.connection.clone(),
        }
    }

    fn manifest_view(submission: &SubmissionId, record: &ManifestRecord) -> StagedManifestState {
        StagedManifestState {
            submission: submission.clone(),
            count: record.count,
            registered: record.transfers.clone(),
            sealed_digest: record.sealed_digest.clone(),
        }
    }

    fn staged_total(state: &StagingState) -> u64 {
        state
            .transfers
            .values()
            .map(|record| match &record.sealed {
                Some(sealed) => sealed.length,
                None => record.total_length,
            })
            .fold(0_u64, u64::saturating_add)
    }

    /// 校验并返回已封存清单的认领事实，同时把清单标记为 `claiming`（阻止取消/清理）。
    ///
    /// 首次提交事务必须在提交成功后调用 [`Self::finish_claim`]；失败时用 `committed = false` 解除标记，
    /// 清单仍可被显式重试——**包括**上一次事务已经把字节 rename 到正式内容寻址路径的情况（D8：首条
    /// 保存事务失败后同页同 ID 可重试）。
    pub(crate) fn begin_claim(
        &self,
        claim: &StagedClaim,
    ) -> Result<Vec<ClaimedAttachment>, PortError> {
        let mut state = lock(&self.state);
        let (binding, sealed_digest, claiming, transfers) = {
            let manifest = state.manifests.get(&claim.submission).ok_or_else(|| {
                PortError::NotFound(EntityRef::Submission(claim.submission.clone()))
            })?;
            (
                manifest.binding.clone(),
                manifest.sealed_digest.clone(),
                manifest.claiming,
                manifest.transfers.clone(),
            )
        };
        if !Self::binding_matches(&binding, claim) {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        if claiming {
            return Err(PortError::Conflict(ConflictKind::AlreadyClaimed));
        }
        if sealed_digest.is_none() {
            return Err(PortError::InvalidRequest(
                "preparation manifest is not sealed",
            ));
        }
        let mut out = Vec::with_capacity(transfers.len());
        for transfer_id in &transfers {
            let record = state
                .transfers
                .get(transfer_id)
                .ok_or_else(|| PortError::NotFound(EntityRef::Attachment(transfer_id.clone())))?;
            let Some(sealed) = &record.sealed else {
                return Err(PortError::InvalidRequest(
                    "preparation manifest contains an unsealed transfer",
                ));
            };
            // 字节可能仍在封存路径，也可能已被上一次（回滚的）提交 rename 到正式路径：两种都可认领。
            if !self.sealed_path(&sealed.sha256).is_file()
                && !self.owned_bytes_path(&sealed.sha256).is_file()
            {
                return Err(PortError::Corrupt("staged attachment bytes are missing"));
            }
            out.push(ClaimedAttachment {
                // 内容寻址的正式附件行在首次提交事务里按 sha256 解析/复用；这里没有 DB 行。
                attachment: transfer_id.clone(),
                length: sealed.length,
                sha256: sealed.sha256.clone(),
                media_type: record.media_type.clone(),
                file_name: record.file_name.clone(),
            });
        }
        let Some(manifest) = state.manifests.get_mut(&claim.submission) else {
            return Err(PortError::Corrupt("preparation manifest vanished"));
        };
        manifest.claiming = true;
        Ok(out)
    }

    /// 结束认领。
    ///
    /// - `committed = true`：字节已由正式附件行引用，清除该清单与它名下的运行期传输记录（否则
    ///   `staged_total` 会把已转正的字节永久计入暂存预算）。
    /// - `committed = false`：撤销本次认领，并把**本次认领真的 rename 过**的字节移回封存路径，让失败
    ///   事务可以原样重试（D8 第 2 行「同页同 ID 可重试」）。正式路径上原本就存在的字节（属于更早已
    ///   提交的会话）绝不移回：`begin_claim` 接受该路径不代表本次认领创建了它。
    pub(crate) fn finish_claim(&self, submission: &SubmissionId, committed: bool) {
        let mut state = lock(&self.state);
        let Some(manifest) = state.manifests.get(submission) else {
            return;
        };
        let transfers = manifest.transfers.clone();
        if committed {
            state.manifests.remove(submission);
            for transfer in &transfers {
                // 同一已封存传输可以被另一份清单重复引用：只要还有清单登记它，运行期记录就保留
                // （否则那份清单会变成不可认领）。
                let still_registered = state
                    .manifests
                    .values()
                    .any(|manifest| manifest.transfers.contains(transfer));
                if !still_registered {
                    state.transfers.remove(transfer);
                }
            }
            return;
        }
        let renamed = match state.manifests.get_mut(submission) {
            Some(manifest) => {
                manifest.claiming = false;
                std::mem::take(&mut manifest.renamed)
            }
            None => return,
        };
        drop(state);
        for digest in renamed {
            let sealed = self.sealed_path(&digest);
            let owned = self.owned_bytes_path(&digest);
            if !sealed.exists() && owned.is_file() {
                let _ = std::fs::rename(&owned, &sealed);
            }
        }
    }

    /// 把本次认领的一个摘要从封存路径 rename 到正式内容寻址路径（首次提交认领用）。
    ///
    /// 只有**本次认领真的搬走**了字节时才记入清单的 `renamed`：目标路径已存在（例如更早已提交的会话
    /// 用了同一份内容）时什么也不做，因此失败回滚不会碰它。
    pub(crate) fn claim_bytes(
        &self,
        submission: &SubmissionId,
        digest: &Digest,
    ) -> Result<(), PortError> {
        let target = self.owned_bytes_path(digest);
        if target.exists() {
            return Ok(());
        }
        {
            let state = lock(&self.state);
            let manifest = state
                .manifests
                .get(submission)
                .ok_or_else(|| PortError::Corrupt("preparation manifest vanished"))?;
            if !manifest.claiming {
                return Err(PortError::InvalidRequest(
                    "preparation manifest is not claimed",
                ));
            }
        }
        let source = self.sealed_path(digest);
        std::fs::rename(&source, &target).map_err(|_| io_error())?;
        crate::migrate::tighten_file_permissions(&target).map_err(|_| io_error())?;
        let mut state = lock(&self.state);
        if let Some(manifest) = state.manifests.get_mut(submission) {
            manifest.renamed.push(digest.clone());
        }
        Ok(())
    }
}

#[async_trait]
impl PreparationAttachmentStaging for PreparationStaging {
    async fn begin(&self, begin: StagedTransferBegin) -> Result<StagedTransferState, PortError> {
        Self::ensure_dirs(self)?;
        acp_core::model::require_device_subject(&begin.subject, &begin.binding)
            .map_err(|_| PortError::InvalidRequest("preparation target subject mismatch"))?;
        if begin.total_length > self.config.attachment_max_file_bytes {
            return Err(budget_error());
        }
        let mut state = lock(&self.state);
        if let Some(existing) = state.transfers.get_mut(&begin.transfer) {
            let same = existing.binding == begin.binding
                && same_subject(&existing.subject, &begin.subject)
                && existing.total_length == begin.total_length
                && existing.media_type == begin.immutable.media_type
                && existing.file_name == begin.immutable.file_name;
            if !same {
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            // 重绑：只转移连接归属，不重置偏移/已接收字节。
            existing.connection = begin.connection;
            existing.updated_at = begin.at.clone();
            return Ok(self.transfer_view(&begin.transfer, existing));
        }
        let total = Self::staged_total(&state);
        if total.saturating_add(begin.total_length) > self.config.attachment_max_total_bytes {
            return Err(budget_error());
        }
        let path = self.receiving_path(&begin.transfer);
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .map_err(|_| io_error())?;
        crate::migrate::tighten_file_permissions(&path).map_err(|_| io_error())?;
        let record = TransferRecord {
            binding: begin.binding.clone(),
            subject: begin.subject,
            connection: begin.connection,
            total_length: begin.total_length,
            received: 0,
            media_type: begin.immutable.media_type,
            file_name: begin.immutable.file_name,
            state: TransferState::Receiving,
            sealed: None,
            updated_at: begin.at,
        };
        let view = self.transfer_view(&begin.transfer, &record);
        state.transfers.insert(begin.transfer, record);
        Ok(view)
    }

    async fn append(&self, chunk: StagedChunk) -> Result<StagedChunkOutcome, PortError> {
        if chunk.bytes.is_empty() || chunk.bytes.len() > CHUNK_SIZE {
            return Err(PortError::InvalidRequest(
                "chunk size must be 1..=32768 bytes",
            ));
        }
        let mut state = lock(&self.state);
        let record = state
            .transfers
            .get(&chunk.transfer)
            .ok_or_else(|| PortError::NotFound(EntityRef::Attachment(chunk.transfer.clone())))?;
        if record.connection != chunk.connection {
            return Ok(StagedChunkOutcome::ConnectionMismatch);
        }
        if record.state != TransferState::Receiving {
            return Err(PortError::InvalidRequest("transfer is not receiving"));
        }
        let received = record.received;
        let total = record.total_length;
        let path = self.receiving_path(&chunk.transfer);
        if chunk.offset == received {
            let next = received
                .checked_add(chunk.bytes.len() as u64)
                .filter(|next| *next <= total)
                .ok_or(PortError::InvalidRequest(
                    "chunk would exceed the declared total length",
                ))?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|error| match error.kind() {
                    std::io::ErrorKind::NotFound => {
                        PortError::Corrupt("staged attachment bytes are missing")
                    }
                    _ => io_error(),
                })?;
            file.seek(SeekFrom::Start(received))
                .map_err(|_| io_error())?;
            file.write_all(&chunk.bytes).map_err(|_| io_error())?;
            file.flush().map_err(|_| io_error())?;
            let Some(record) = state.transfers.get_mut(&chunk.transfer) else {
                return Err(PortError::Corrupt("transfer vanished"));
            };
            record.received = next;
            record.updated_at = chunk.at;
            return Ok(StagedChunkOutcome::Accepted { next_offset: next });
        }
        if chunk.offset > received {
            return Ok(StagedChunkOutcome::OffsetMismatch {
                expected: received,
                actual: chunk.offset,
            });
        }
        let available = received - chunk.offset;
        if available < chunk.bytes.len() as u64 {
            return Ok(StagedChunkOutcome::OffsetMismatch {
                expected: received,
                actual: chunk.offset,
            });
        }
        let mut file = std::fs::File::open(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                PortError::Corrupt("staged attachment bytes are missing")
            }
            _ => io_error(),
        })?;
        file.seek(SeekFrom::Start(chunk.offset))
            .map_err(|_| io_error())?;
        let mut existing = vec![0_u8; chunk.bytes.len()];
        file.read_exact(&mut existing).map_err(|_| io_error())?;
        if existing == chunk.bytes {
            Ok(StagedChunkOutcome::Duplicate {
                next_offset: received,
            })
        } else {
            Ok(StagedChunkOutcome::ChunkConflict {
                offset: chunk.offset,
            })
        }
    }

    async fn seal(&self, seal: StagedSeal) -> Result<StagedSealOutcome, PortError> {
        let mut state = lock(&self.state);
        let record = state
            .transfers
            .get(&seal.transfer)
            .ok_or_else(|| PortError::NotFound(EntityRef::Attachment(seal.transfer.clone())))?;
        if record.connection != seal.connection {
            return Ok(StagedSealOutcome::ConnectionMismatch);
        }
        if record.state == TransferState::Sealed {
            let Some(sealed) = &record.sealed else {
                return Err(PortError::Corrupt("sealed transfer has no facts"));
            };
            if sealed.sha256 == seal.sha256 && sealed.length == seal.length {
                return Ok(StagedSealOutcome::AlreadySealed(SealedFacts {
                    transfer: seal.transfer.clone(),
                    attachment: seal.transfer.clone(),
                    length: sealed.length,
                    sha256: sealed.sha256.clone(),
                    sealed_at: sealed.sealed_at.clone(),
                }));
            }
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        if record.received != record.total_length || seal.length != record.total_length {
            return Err(PortError::InvalidRequest(
                "sealed length does not match the received bytes",
            ));
        }
        let receiving = self.receiving_path(&seal.transfer);
        let actual = stream_digest(&receiving)?;
        if actual != seal.sha256 {
            return Ok(StagedSealOutcome::DigestMismatch);
        }
        let sealed_path = self.sealed_path(&seal.sha256);
        if sealed_path.exists() {
            // 内容寻址：同样的字节只存一份。
            let _ = std::fs::remove_file(&receiving);
        } else {
            std::fs::rename(&receiving, &sealed_path).map_err(|_| io_error())?;
            crate::migrate::tighten_file_permissions(&sealed_path).map_err(|_| io_error())?;
        }
        let Some(record) = state.transfers.get_mut(&seal.transfer) else {
            return Err(PortError::Corrupt("transfer vanished"));
        };
        record.state = TransferState::Sealed;
        record.sealed = Some(SealedFactsRecord {
            length: seal.length,
            sha256: seal.sha256.clone(),
            sealed_at: seal.at.clone(),
        });
        record.updated_at = seal.at.clone();
        Ok(StagedSealOutcome::Sealed(SealedFacts {
            transfer: seal.transfer.clone(),
            attachment: seal.transfer,
            length: seal.length,
            sha256: seal.sha256,
            sealed_at: seal.at,
        }))
    }

    async fn status(
        &self,
        transfer: AttachmentId,
    ) -> Result<Option<StagedTransferState>, PortError> {
        let state = lock(&self.state);
        Ok(state
            .transfers
            .get(&transfer)
            .map(|record| self.transfer_view(&transfer, record)))
    }

    async fn abort(
        &self,
        transfer: AttachmentId,
        _reason: StagedAbortReason,
    ) -> Result<(), PortError> {
        let mut state = lock(&self.state);
        if state
            .manifests
            .values()
            .any(|manifest| manifest.transfers.contains(&transfer))
        {
            return Err(PortError::Conflict(ConflictKind::AlreadyClaimed));
        }
        let Some(record) = state.transfers.remove(&transfer) else {
            return Ok(());
        };
        let _ = std::fs::remove_file(self.receiving_path(&transfer));
        if let Some(sealed) = record.sealed {
            let digest = sealed.sha256;
            let still_referenced = state.transfers.values().any(|other| {
                other
                    .sealed
                    .as_ref()
                    .is_some_and(|other| other.sha256 == digest)
            });
            if !still_referenced {
                let _ = std::fs::remove_file(self.sealed_path(&digest));
            }
        }
        Ok(())
    }

    async fn begin_manifest(
        &self,
        begin: StagedManifestBegin,
    ) -> Result<StagedManifestState, PortError> {
        acp_core::model::require_device_subject(&begin.subject, &begin.binding)
            .map_err(|_| PortError::InvalidRequest("preparation target subject mismatch"))?;
        if begin.count == 0 || begin.count as usize > MANIFEST_PAGE * MANIFEST_PAGE {
            return Err(PortError::InvalidRequest(
                "manifest count is outside the supported range",
            ));
        }
        let mut state = lock(&self.state);
        if let Some(existing) = state.manifests.get_mut(&begin.submission) {
            if existing.binding != begin.binding
                || !same_subject(&existing.subject, &begin.subject)
                || existing.count != begin.count
            {
                return Err(PortError::Conflict(ConflictKind::AlreadyExists));
            }
            existing.connection = begin.connection;
            existing.updated_at = begin.at;
            return Ok(Self::manifest_view(&begin.submission, existing));
        }
        let record = ManifestRecord {
            binding: begin.binding,
            subject: begin.subject,
            connection: begin.connection,
            count: begin.count,
            transfers: Vec::new(),
            sealed_digest: None,
            claiming: false,
            renamed: Vec::new(),
            updated_at: begin.at,
        };
        let view = Self::manifest_view(&begin.submission, &record);
        state.manifests.insert(begin.submission, record);
        Ok(view)
    }

    async fn append_manifest_page(
        &self,
        page: StagedManifestPage,
    ) -> Result<StagedManifestState, PortError> {
        if page.transfers.is_empty() || page.transfers.len() > MANIFEST_PAGE {
            return Err(PortError::InvalidRequest(
                "a manifest page must carry 1..=64 transfers",
            ));
        }
        let mut state = lock(&self.state);
        let manifest = state
            .manifests
            .get(&page.submission)
            .ok_or_else(|| PortError::NotFound(EntityRef::Submission(page.submission.clone())))?;
        if manifest.connection != page.connection {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        if manifest.sealed_digest.is_some() {
            let already = page
                .transfers
                .iter()
                .all(|transfer| manifest.transfers.contains(transfer));
            if already {
                return Ok(Self::manifest_view(&page.submission, manifest));
            }
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        // 同一页重放：已按同一顺序登记过。
        let registered = manifest.transfers.len();
        if registered >= page.transfers.len() {
            let offset = registered - page.transfers.len();
            if manifest.transfers[offset..] == page.transfers {
                return Ok(Self::manifest_view(&page.submission, manifest));
            }
        }
        if registered + page.transfers.len() > manifest.count as usize {
            return Err(PortError::InvalidRequest(
                "manifest page exceeds the declared count",
            ));
        }
        for transfer in &page.transfers {
            let record = state
                .transfers
                .get(transfer)
                .ok_or_else(|| PortError::NotFound(EntityRef::Attachment(transfer.clone())))?;
            if record.binding != manifest.binding
                || !same_subject(&record.subject, &manifest.subject)
                || record.sealed.is_none()
            {
                return Err(PortError::InvalidRequest(
                    "manifest page references an unsealed or foreign transfer",
                ));
            }
        }
        let Some(manifest) = state.manifests.get_mut(&page.submission) else {
            return Err(PortError::Corrupt("preparation manifest vanished"));
        };
        manifest.transfers.extend(page.transfers);
        manifest.updated_at = page.at;
        Ok(Self::manifest_view(&page.submission, manifest))
    }

    async fn seal_manifest(
        &self,
        seal: StagedManifestSeal,
    ) -> Result<StagedManifestState, PortError> {
        let mut state = lock(&self.state);
        let manifest = state
            .manifests
            .get_mut(&seal.submission)
            .ok_or_else(|| PortError::NotFound(EntityRef::Submission(seal.submission.clone())))?;
        if manifest.connection != seal.connection {
            return Err(PortError::Conflict(ConflictKind::AlreadyExists));
        }
        if manifest.sealed_digest.is_some() {
            return Ok(Self::manifest_view(&seal.submission, manifest));
        }
        if manifest.transfers.len() != manifest.count as usize {
            return Err(PortError::InvalidRequest(
                "manifest does not contain the declared number of transfers",
            ));
        }
        manifest.sealed_digest = Some(manifest_digest(&manifest.transfers));
        manifest.updated_at = seal.at;
        Ok(Self::manifest_view(&seal.submission, manifest))
    }

    async fn claim(&self, claim: StagedClaim) -> Result<Vec<ClaimedAttachment>, PortError> {
        let attachments = self.begin_claim(&claim)?;
        self.finish_claim(&claim.submission, true);
        Ok(attachments)
    }

    async fn release_orphans(&self, before: Timestamp) -> Result<usize, PortError> {
        let mut removed = 0_usize;
        let before_system = system_time(&before);
        {
            let mut state = lock(&self.state);
            // 已封存清单引用的字节、以及窗口内封存的字节一律保留。
            let mut protected: HashSet<String> = HashSet::new();
            for manifest in state.manifests.values() {
                if manifest.claiming {
                    for transfer in &manifest.transfers {
                        if let Some(record) = state.transfers.get(transfer)
                            && let Some(sealed) = &record.sealed
                        {
                            protected.insert(sealed.sha256.as_str().to_owned());
                        }
                    }
                }
            }
            for record in state.transfers.values() {
                if let Some(sealed) = &record.sealed
                    && sealed.sealed_at >= before
                {
                    protected.insert(sealed.sha256.as_str().to_owned());
                }
            }
            state.transfers.retain(|_, record| {
                record.updated_at >= before
                    || record
                        .sealed
                        .as_ref()
                        .is_some_and(|sealed| protected.contains(sealed.sha256.as_str()))
            });
            state
                .manifests
                .retain(|_, manifest| manifest.claiming || manifest.updated_at >= before);
            // 清理已没有任何运行期记录的接收文件。
            if let Ok(entries) = std::fs::read_dir(self.receiving_dir()) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && older_than(&path, before_system) {
                        let referenced = state
                            .transfers
                            .keys()
                            .any(|transfer| self.receiving_path(transfer) == path);
                        if !referenced && std::fs::remove_file(&path).is_ok() {
                            removed += 1;
                        }
                    }
                }
            }
        }
        let protected = {
            let state = lock(&self.state);
            let mut set: HashSet<String> = HashSet::new();
            for manifest in state.manifests.values() {
                if manifest.claiming {
                    for transfer in &manifest.transfers {
                        if let Some(record) = state.transfers.get(transfer)
                            && let Some(sealed) = &record.sealed
                        {
                            set.insert(sealed.sha256.as_str().to_owned());
                        }
                    }
                }
            }
            for record in state.transfers.values() {
                if let Some(sealed) = &record.sealed
                    && sealed.sealed_at >= before
                {
                    set.insert(sealed.sha256.as_str().to_owned());
                }
            }
            set
        };
        if let Ok(entries) = std::fs::read_dir(self.sealed_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if path.is_file()
                    && !protected.contains(name)
                    && older_than(&path, before_system)
                    && std::fs::remove_file(&path).is_ok()
                {
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }
}

/// 组合根持有 `SqliteStore` 即可使用暂存端口（与首条事务共用同一个运行期目录）。
#[async_trait]
impl PreparationAttachmentStaging for crate::session_store::SqliteStore {
    async fn begin(&self, begin: StagedTransferBegin) -> Result<StagedTransferState, PortError> {
        self.staging.begin(begin).await
    }

    async fn append(&self, chunk: StagedChunk) -> Result<StagedChunkOutcome, PortError> {
        self.staging.append(chunk).await
    }

    async fn seal(&self, seal: StagedSeal) -> Result<StagedSealOutcome, PortError> {
        self.staging.seal(seal).await
    }

    async fn status(
        &self,
        transfer: AttachmentId,
    ) -> Result<Option<StagedTransferState>, PortError> {
        self.staging.status(transfer).await
    }

    async fn abort(
        &self,
        transfer: AttachmentId,
        reason: StagedAbortReason,
    ) -> Result<(), PortError> {
        self.staging.abort(transfer, reason).await
    }

    async fn begin_manifest(
        &self,
        begin: StagedManifestBegin,
    ) -> Result<StagedManifestState, PortError> {
        self.staging.begin_manifest(begin).await
    }

    async fn append_manifest_page(
        &self,
        page: StagedManifestPage,
    ) -> Result<StagedManifestState, PortError> {
        self.staging.append_manifest_page(page).await
    }

    async fn seal_manifest(
        &self,
        seal: StagedManifestSeal,
    ) -> Result<StagedManifestState, PortError> {
        self.staging.seal_manifest(seal).await
    }

    async fn claim(&self, claim: StagedClaim) -> Result<Vec<ClaimedAttachment>, PortError> {
        self.staging.claim(claim).await
    }

    async fn release_orphans(&self, before: Timestamp) -> Result<usize, PortError> {
        self.staging.release_orphans(before).await
    }
}
