//! 准备运行期目录与准备附件暂存的纯类型（`docs/CORE_PORTS_AND_STORAGE.md` §3.10/§5.11）。
//!
//! 准备是**运行期**事实（`design.md` D1）：本模块不落库、不产生事件、不登记目录、不写命令收据。
//! 状态机 `allocated → preparing → ready → released` 只在 [`crate::preparation::PreparationDirectory`]
//! 的内存目录里推进；失败是带原因的取值（[`PreparationFailure`]），不是第五个隐式状态。
//!
//! 暂存端口只维护运行期传输/清单元数据与 Owner 私有内容区中的字节；这里只定义形状与不变量，
//! 不带路径、SQL 或平台类型。

use super::error::InvalidValue;
use super::ids::{
    AgentRef, AttachmentId, ConnectionId, DeviceId, DirectoryId, RuntimeEpoch, SessionId,
    SubmissionId,
};
use super::media::AttachmentSubject;
use super::scalars::{Digest, Nonce, ResourceGeneration, Timestamp};

/// 准备查看租约的固定 v1 时长（30 秒，`design.md` D3；不是可配置值）。
pub const PREPARATION_LEASE_MS: u64 = 30_000;
/// 准备租约的续租节奏（可见时每 10 秒一次；租约仍是 30 秒）。
pub const PREPARATION_RENEW_INTERVAL_MS: u64 = 10_000;
/// 已释放事实的内存上界（拒绝旧代操作；超出后最旧的先被遗忘）。
pub const PREPARATION_RELEASED_TOMBSTONES: usize = 1_024;

/// 准备资源状态（`design.md` D1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreparationState {
    /// ID 已由 Owner 分配，尚未建立临时端点。
    Allocated,
    /// 正在建立/重建临时端点并确认真实设置。
    Preparing,
    /// 端点与设置已确认，可以接受首条。
    Ready,
    /// 已释放（含确认释放与显式离开）；只保留有界的释放事实。
    Released,
}

impl PreparationState {
    /// 全部取值。
    pub const ALL: [Self; 4] = [
        Self::Allocated,
        Self::Preparing,
        Self::Ready,
        Self::Released,
    ];

    /// 稳定 token（只用于诊断/测试，不是 wire 字段）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allocated => "allocated",
            Self::Preparing => "preparing",
            Self::Ready => "ready",
            Self::Released => "released",
        }
    }

    /// 是否占用一个运行期准备名额。
    pub const fn occupies_capacity(self) -> bool {
        !matches!(self, Self::Released)
    }
}

impl std::fmt::Display for PreparationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 准备失败的具名原因。失败**不**伪造 `ready`，也不靠删除目录项假装释放。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreparationFailureReason {
    /// 临时端点未能建立/重建（进程、能力或关闭失败）。
    EndpointUnavailable,
    /// 配置复核失败：草稿选择在新端点不可用；保留选择并阻止发送。
    ConfigurationRejected,
    /// 租约到期且没有可续租的查看者。
    LeaseExpired,
    /// 同节点运行资源已满，且没有可安全回收的候选。
    CapacityExceeded,
    /// 释放结果不确定/失败：名额继续占用。
    ReleaseUncertain,
}

impl PreparationFailureReason {
    /// 全部取值。
    pub const ALL: [Self; 5] = [
        Self::EndpointUnavailable,
        Self::ConfigurationRejected,
        Self::LeaseExpired,
        Self::CapacityExceeded,
        Self::ReleaseUncertain,
    ];

    /// 稳定 token（诊断/测试）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EndpointUnavailable => "endpoint_unavailable",
            Self::ConfigurationRejected => "configuration_rejected",
            Self::LeaseExpired => "lease_expired",
            Self::CapacityExceeded => "capacity_exceeded",
            Self::ReleaseUncertain => "release_uncertain",
        }
    }
}

impl std::fmt::Display for PreparationFailureReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 一次准备失败的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationFailure {
    pub reason: PreparationFailureReason,
}

/// 准备归属：已认证设备 + 标签页归属（`tabNonce` 从不进入 URL）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationOwner {
    pub device: DeviceId,
    pub tab_nonce: Nonce,
}

impl PreparationOwner {
    /// 构造。
    pub fn new(device: DeviceId, tab_nonce: Nonce) -> Self {
        Self { device, tab_nonce }
    }
}

/// 分配输入：只含 Agent、目录与标签页归属（模式/配置在资源代上另行确认）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocatePreparation {
    pub agent: AgentRef,
    pub directory: Option<DirectoryId>,
    pub owner: PreparationOwner,
}

/// 可投影的准备状态（运行期值，不落库）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationView {
    pub session: SessionId,
    pub runtime_epoch: RuntimeEpoch,
    pub generation: ResourceGeneration,
    pub state: PreparationState,
    pub agent: AgentRef,
    pub directory: Option<DirectoryId>,
    pub owner: PreparationOwner,
    pub lease_expires_at: Option<Timestamp>,
    pub failure: Option<PreparationFailure>,
}

impl PreparationView {
    /// 该准备是否可接受首条（`ready` 且未冻结）。
    pub fn is_ready(&self) -> bool {
        self.state == PreparationState::Ready
    }
}

/// 分配结果：容量不足是显式的（适配层映射 `session.capacity_exceeded`），不是空占位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocatePreparationOutcome {
    Allocated(PreparationView),
    CapacityExceeded,
}

/// `stale` 的唯一来源（`configure/renew/release` 与附件 begin/manifest）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreparationStaleReason {
    RuntimeEpochMismatch,
    GenerationMismatch,
    Released,
}

impl PreparationStaleReason {
    /// 稳定 token（wire 的 `reason` 取值，`design.md` D3.1）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RuntimeEpochMismatch => "runtime_epoch_mismatch",
            Self::GenerationMismatch => "generation_mismatch",
            Self::Released => "released",
        }
    }
}

impl std::fmt::Display for PreparationStaleReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 准备操作的运行期判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparationDecision {
    Ready(PreparationView),
    Stale(PreparationStaleReason),
    /// 显式释放成功或幂等成功。
    Released,
    /// 目录里没有该准备（未知 ID；不得启动替代资源）。
    Unknown,
}

/// 显式释放的结果。`Failed`/`Uncertain` **保留**容量占位与资源事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparationReleaseOutcome {
    Released,
    Failed,
    Uncertain,
}

/// 首次受理的冻结结果：在真正提交/派发前固定资源代，避免租约到期与提交并发释放正要派发的端点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparationFreeze {
    Granted(PreparationGrant),
    /// 已有另一次提交在冻结/提交中，或正在释放。
    Busy,
    Stale(PreparationStaleReason),
    Unknown,
}

/// 已冻结的准备资源：提交时必须原样使用这个资源代与 Agent 身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationGrant {
    pub session: SessionId,
    pub runtime_epoch: RuntimeEpoch,
    pub generation: ResourceGeneration,
    pub agent: AgentRef,
    pub directory: Option<DirectoryId>,
    pub owner: PreparationOwner,
}

/// 准备目标的归属绑定：由**已核验**的连接身份与准备资源恢复，不来自请求字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationBinding {
    pub device: DeviceId,
    pub session: SessionId,
    pub tab_nonce: Nonce,
    pub runtime_epoch: RuntimeEpoch,
    pub generation: ResourceGeneration,
    pub claims_digest: Digest,
}

/// 不可变的文件声明（同一 transferId 的不同声明返回绑定冲突；不重置偏移）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmutableFileMetadata {
    pub media_type: String,
    pub file_name: String,
}

/// 开始一次准备附件传输（运行期元数据 + 私有内容区字节；不写任何数据库行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedTransferBegin {
    pub transfer: AttachmentId,
    pub binding: PreparationBinding,
    /// 被校验的声明；必须是 `Device(binding.device)`。
    pub subject: AttachmentSubject,
    pub connection: ConnectionId,
    pub total_length: u64,
    pub immutable: ImmutableFileMetadata,
    pub at: Timestamp,
}

/// 暂存传输的读取投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedTransferState {
    pub transfer: AttachmentId,
    pub binding: PreparationBinding,
    pub received: u64,
    pub total_length: u64,
    pub sealed: Option<SealedFacts>,
    pub bound_connection: ConnectionId,
}

/// 追加一块。`bytes` 不得超过 `ATTACHMENT_CHUNK_SIZE`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedChunk {
    pub transfer: AttachmentId,
    pub connection: ConnectionId,
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub at: Timestamp,
}

/// 块确认结果（与既有 `attachment.chunk_ack` 语义一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagedChunkOutcome {
    Accepted {
        next_offset: u64,
    },
    Duplicate {
        next_offset: u64,
    },
    OffsetMismatch {
        expected: u64,
        actual: u64,
    },
    ChunkConflict {
        offset: u64,
    },
    /// 未重绑/跨连接：映射 `attachment.binding_mismatch`。
    ConnectionMismatch,
}

/// 封存：校验完整长度与流式 SHA-256，封存后字节不可变。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedSeal {
    pub transfer: AttachmentId,
    pub connection: ConnectionId,
    pub length: u64,
    pub sha256: Digest,
    pub at: Timestamp,
}

/// 封存的不可变事实；`attachment` 在认领前**不是**正式消息引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedFacts {
    pub transfer: AttachmentId,
    pub attachment: AttachmentId,
    pub length: u64,
    pub sha256: Digest,
    pub sealed_at: Timestamp,
}

/// 封存结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagedSealOutcome {
    Sealed(SealedFacts),
    /// 重复封存相同内容幂等返回原事实；不同内容返回冲突。
    AlreadySealed(SealedFacts),
    DigestMismatch,
    ConnectionMismatch,
}

/// 取消原因（只影响暂存元数据与私有字节，不产生正式事件）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StagedAbortReason {
    ClientCancelled,
    BudgetExceeded,
    Revoked,
    DigestMismatch,
}

impl StagedAbortReason {
    /// 全部取值。
    pub const ALL: [Self; 4] = [
        Self::ClientCancelled,
        Self::BudgetExceeded,
        Self::Revoked,
        Self::DigestMismatch,
    ];

    /// 稳定 token（诊断）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClientCancelled => "client_cancelled",
            Self::BudgetExceeded => "budget_exceeded",
            Self::Revoked => "revoked",
            Self::DigestMismatch => "digest_mismatch",
        }
    }
}

/// 开始一份准备清单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedManifestBegin {
    pub submission: SubmissionId,
    pub binding: PreparationBinding,
    pub subject: AttachmentSubject,
    pub connection: ConnectionId,
    pub count: u32,
    pub at: Timestamp,
}

/// 一页清单（顺序即认领顺序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedManifestPage {
    pub submission: SubmissionId,
    pub connection: ConnectionId,
    pub transfers: Vec<AttachmentId>,
    pub final_page: bool,
    pub at: Timestamp,
}

/// 封存清单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedManifestSeal {
    pub submission: SubmissionId,
    pub connection: ConnectionId,
    pub at: Timestamp,
}

/// 清单状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedManifestState {
    pub submission: SubmissionId,
    pub count: u32,
    pub registered: Vec<AttachmentId>,
    pub sealed_digest: Option<Digest>,
}

impl StagedManifestState {
    /// 是否已完整封存。
    pub fn is_sealed(&self) -> bool {
        self.sealed_digest.is_some()
    }
}

/// 首次提交的认领请求：只核验已封存事实并原子认领元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedClaim {
    pub device: DeviceId,
    pub session: SessionId,
    pub submission: SubmissionId,
    pub runtime_epoch: RuntimeEpoch,
    pub generation: ResourceGeneration,
}

/// 认领到的附件事实（内容字节已在同一受管内容区，不再复制或全量重哈希）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedAttachment {
    pub attachment: AttachmentId,
    pub length: u64,
    pub sha256: Digest,
    pub media_type: String,
    pub file_name: String,
}

/// 解析 `source`：只接受 `Device(device)` 且与绑定一致的主体；节点主体不参与准备暂存。
pub fn require_device_subject(
    subject: &AttachmentSubject,
    binding: &PreparationBinding,
) -> Result<(), InvalidValue> {
    match subject {
        AttachmentSubject::Device(device) if device == &binding.device => Ok(()),
        _ => Err(InvalidValue::Field),
    }
}

#[cfg(test)]
#[path = "preparation_tests.rs"]
mod tests;
