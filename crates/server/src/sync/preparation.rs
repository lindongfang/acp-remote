//! 准备控制面（`core.session-preparation.v1`，`docs/SYNC_PROTOCOL.md` §11.9）：
//! `session.preparation`/`session.preparation.result` 的帧分派、凭据验证与运行期目录编排。
//!
//! 五条边界（`design.md` D2/D3/D3.1/D6/D9）：
//!
//! - **认证先于一切**：每条请求先读该设备的持久信任行（[`PreparationTrust::snapshot`]）；撤销/未知
//!   设备立即按 §8 的认证失败处置，不读准备目录、不建端点、不回显任何资源事实。准备凭据
//!   （receipt）只是**节点签发的分配证明**，不是登录凭证，也不能替代逐连接认证。
//! - **feature 门控**：未协商 `core.session-preparation.v1` 时回 `protocol.type_unsupported`，判定在
//!   任何准备目录/端点调用之前（失败关闭）。
//! - **运行期事实**：分配、资源代、租约与释放都在 [`PreparationDirectory`]（core 的纯运行期目录）；
//!   本模块不写 owned 行、不写命令收据、不发事件。分配成功只回发起连接。
//! - **连接期去重**：`operationId` 的去重窗口属于**当前连接**（重连后重新开始）。同 ID 同参数幂等
//!   回放原结果；同 ID 不同参数按 `command.idempotency_conflict` 拒绝，且不执行第二次动作。
//! - **判断优先级固定**：当前连接认证/撤销 → 凭据签名与节点/origin/storeEpoch/设备/ID/标签页绑定 →
//!   同 ID 首条持久受理查询（`committed` 优先于旧代/租约/目录失效）→ 仅未受理时核对
//!   runtimeEpoch/generation/lease 与准备现状 → 执行动作。
//!
//! `allocate` 在 ID 尚未分配时失败（目录解析、容量）没有 `sessionId` 可填进闭合结果，因此走**帧级**
//! `error`（`correlationId` 是该请求帧的 `messageId`）；一旦 ID 已分配，后续失败一律用 `status=failed`
//! 的闭合结果，`sessionId` 因此永远来自 Owner 而不是客户端自报值。
//!
//! 依赖纪律：只调用 `core` 的公开面（`PreparationDirectory`、`FirstSubmission`、`UseCases` 的只读用例）
//! 与 `identity-auth` 的公开验签/签发入口；不 import `storage-sqlite`、不调用 `agent-host`（临时端点由
//! 组合根以 [`PreparationEndpoints`] 注入），也不调用平级 adapter。

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use acp_core::model::{
    Actor, AgentId, AgentRef, AllocatePreparation, AllocatePreparationOutcome, ConfigOption,
    ConfigOptionId, ConfigValue, DeviceId, Digest, FirstSubmissionId, FirstSubmissionRecord,
    ModeId, ModeState, NodeId, Nonce, PortError, PreparationDecision, PreparationFailureReason,
    PreparationOwner, PreparationReleaseOutcome, PreparationStaleReason, PreparationState,
    PreparationView, ResolvedWorkspace, ResourceGeneration, RuntimeEpoch, ServerEpoch, SessionId,
    Timestamp, UnavailableKind,
};
use acp_core::ports::{FirstSubmission, PreparationAttachmentStaging};
use acp_core::preparation::PreparationDirectory;
use acp_core::use_cases::{PreparationWorkspace, PreparationWorkspaceSource, UseCases};
use async_trait::async_trait;
use identity_auth::{
    Authenticated, Authority, CanonicalOrigin, ConnectionBinding, CredentialStatus, IdentityFact,
    NormalizedWorkspacePath, P1363Signature, PeerPublicKey, PreparationClaims, PreparationError,
    PreparationExpectation, PreparationReceipt,
    PreparationWorkspaceSource as ReceiptWorkspaceSource, verify_preparation_receipt,
};
use sync_protocol::common::{
    Base64Url, ConfigOptionValue as WireConfigValue, NonEmptyText, Nullable, ProtocolVersionV1,
    RawObject, Text, Timestamp as WireTimestamp, Uuid,
};
use sync_protocol::error::ErrorCode;
use sync_protocol::sync::{
    PreparationOutcome, PreparationRequestDetail, PreparationStaleReason as WireStaleReason,
    SessionPreparation, SessionPreparationResult,
};
use tracing::debug;

use crate::sync::conn::handshake::SUPPORTED_FEATURES;
use crate::sync::conn::{ConnectionHandle, MessageRoute, RouteOutcome, close};
use crate::transport::net::ratelimit::MAX_TRACKED_KEYS;

/// `core.session-preparation.v1`（`compatibility/features/v1/features.json`）。
pub const PREPARATION_FEATURE: &str = "core.session-preparation.v1";

/// 单连接准备控制帧速率（§11.9/§14：120 帧/分钟，只统计客户端 → 服务端的 `session.preparation`）。
pub const PREPARATIONS_PER_MINUTE: u32 = 120;
/// 控制帧的限流窗口（固定 1 分钟）。
pub const PREPARATION_WINDOW: Duration = Duration::from_secs(60);
/// 连接期去重表的条数上限（有界，不随页面长期运行增长）。
pub const MAX_DEDUP_ENTRIES: usize = 64;

/// 本 feature 是否已在静态支持表里登记（组合根在端口齐备时另行追加宣告；见模块文档）。
///
/// 它不修改 [`SUPPORTED_FEATURES`]：该表与 registry 的 `delivery = mvp` 子集逐条相等，而本 feature 是
/// `conditional_mvp`，必须由组合根按端口可用性单独追加。
pub fn feature_supported_by_build() -> bool {
    SUPPORTED_FEATURES.contains(&PREPARATION_FEATURE)
}

// ---------------------------------------------------------------------------------------------
// 组合根注入的窄面：持久信任 + 节点密钥 + 目录解析
// ---------------------------------------------------------------------------------------------

/// 准备控制/附件面需要的持久事实与节点签名入口（组合根用 `UseCases` + `Authority` 装配）。
///
/// 拆成窄 trait 而不是直接持有 `Arc<UseCases>`：控制面只需要「设备快照 / 目录解析 / 节点密钥 /
/// 签发凭据 / 时钟」五件事，测试因此可以注入确定性的实现而不必装配整个用例面。
#[async_trait]
pub trait PreparationTrust: Send + Sync {
    /// 该设备的当次持久快照（凭据状态 + 本机 storeEpoch + 本节点标识）。
    async fn snapshot(&self, device: &DeviceId) -> Result<TrustSnapshot, PortError>;

    /// 本节点身份公钥（验签唯一来源；绝不来自对端消息）。
    async fn node_public_key(&self) -> Result<PeerPublicKey, PortError>;

    /// 用本节点身份密钥签一组准备 claims（claims 的 `ownerNodeId` 必须等于本节点）。
    async fn sign(&self, claims: &PreparationClaims) -> Result<PreparationReceipt, PortError>;

    /// 准备目录解析（与 `session.create` 共用同一套形状/失败分类）。
    async fn resolve_workspace(
        &self,
        actor: &Actor,
        source: PreparationWorkspaceSource,
    ) -> Result<PreparationWorkspace, PortError>;

    /// 注入时钟（`identity-auth` 与用例面共用同一个来源）。
    fn now(&self) -> Timestamp;
}

/// 持久信任快照：只含 receipt 验证需要的三件事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustSnapshot {
    /// 本节点标识（claims 的 `ownerNodeId`）。
    pub local_node: NodeId,
    /// 持久 `meta.server_epoch`（正常重启不变；整体重建存储时改变）。
    pub server_epoch: ServerEpoch,
    /// 设备凭据状态（`Active` 之外的取值一律按认证失败处置）。
    pub credential: CredentialStatus,
}

/// `UseCases` + `Authority` 的生产实现（组合根装配）。
pub struct CorePreparationTrust {
    core: Arc<UseCases>,
    authority: Arc<Authority>,
}

impl CorePreparationTrust {
    /// 装配。
    pub fn new(core: Arc<UseCases>, authority: Arc<Authority>) -> Self {
        Self { core, authority }
    }
}

#[async_trait]
impl PreparationTrust for CorePreparationTrust {
    async fn snapshot(&self, device: &DeviceId) -> Result<TrustSnapshot, PortError> {
        let view = self.core.sync_device_handshake_view(device).await?;
        let credential = match view
            .device
            .as_ref()
            .map(acp_core::model::DeviceRecord::state)
        {
            Some(acp_core::model::DeviceState::Active) => CredentialStatus::Active,
            Some(acp_core::model::DeviceState::Revoked) => CredentialStatus::Revoked,
            Some(acp_core::model::DeviceState::Pending) | None => CredentialStatus::Unknown,
        };
        Ok(TrustSnapshot {
            local_node: self.authority.local_node().clone(),
            server_epoch: view.head.server_epoch,
            credential,
        })
    }

    async fn node_public_key(&self) -> Result<PeerPublicKey, PortError> {
        self.authority
            .node_public_key()
            .await
            .map_err(|_| PortError::Unavailable(UnavailableKind::KeystoreUnavailable))
    }

    async fn sign(&self, claims: &PreparationClaims) -> Result<PreparationReceipt, PortError> {
        self.authority
            .sign_preparation_receipt(claims)
            .await
            .map_err(|_| PortError::Unavailable(UnavailableKind::KeystoreUnavailable))
    }

    async fn resolve_workspace(
        &self,
        actor: &Actor,
        source: PreparationWorkspaceSource,
    ) -> Result<PreparationWorkspace, PortError> {
        self.core.resolve_preparation_workspace(actor, source).await
    }

    fn now(&self) -> Timestamp {
        self.authority.now()
    }
}

// ---------------------------------------------------------------------------------------------
// 临时端点的窄面（组合根用 `agent-host` 的临时端点实现；本 crate 不依赖它）
// ---------------------------------------------------------------------------------------------

/// 建立/重建准备端点并复核设置所需的输入。
pub struct PreparationEstablish {
    /// Owner 分配的会话 ID。
    pub session: SessionId,
    /// 已确认的 Agent。
    pub agent: AgentRef,
    /// 已解析的工作目录（路径与目录标识）。
    pub workspace: ResolvedWorkspace,
    /// 当前 core 资源代（适配器可用于代隔离日志，不参与 Agent 侧代际规则）。
    pub generation: u64,
    /// 要复核/应用的草稿选择（`allocate`/`restore` 为空）。
    pub selection: PreparationSelectionInput,
}

/// 在当前资源代上复核/应用设置。
pub struct PreparationConfigure {
    /// 会话 ID。
    pub session: SessionId,
    /// core 资源代（必须与准备目录的当前代一致，调用方已判定）。
    pub generation: u64,
    /// 草稿选择；空 = 只读当前选项。
    pub selection: PreparationSelectionInput,
}

/// 准备阶段的草稿选择（已从 wire 折成 core 值对象）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreparationSelectionInput {
    /// 要应用的模式。
    pub mode: Option<ModeId>,
    /// 要应用的配置项（精确值）。
    pub config: Vec<(ConfigOptionId, ConfigValue)>,
}

/// Agent 当前**真实**的选项（应用后重新读取）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparationOptions {
    /// 模式状态。
    pub modes: ModeState,
    /// 配置项。
    pub config: Vec<ConfigOption>,
    /// 是否存在被 Agent 拒绝的草稿选择（`true` 时不得派发首条：保留选择并阻止发送，`design.md` D4）。
    pub blocks_dispatch: bool,
}

/// 临时端点的释放结果。`Failed`/`Uncertain` **保留**容量占位（关闭失败/未知照旧占位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointRelease {
    /// 已确认释放（幂等）。
    Released,
    /// 说不清是否释放。
    Uncertain,
    /// 明确失败（端点仍可能占用）。
    Failed,
}

/// 临时端点动作的窄面（`configure` 与 `release` 只作用于同一资源代）。
#[async_trait]
pub trait PreparationEndpoints: Send + Sync {
    /// 建立或重建临时端点并复核/应用选择，返回 Agent 确认过的真实选项。
    async fn establish(
        &self,
        request: PreparationEstablish,
    ) -> Result<PreparationOptions, PortError>;

    /// 在当前资源代上复核/应用设置（空选择 = 只读当前选项）。
    async fn configure(
        &self,
        request: PreparationConfigure,
    ) -> Result<PreparationOptions, PortError>;

    /// 把已建立的准备端点转正为正式所有权，返回同一端点用于首次派发（`design.md` D1）。
    ///
    /// 只认精确资源代：旧代句柄、已释放句柄与「目录里没有该代」都必须显式失败，不能转正另一份资源。
    /// 调用方在失败时负责 `abort_submission` / 结算，**不**自动重派。
    async fn claim(
        &self,
        session: SessionId,
        generation: u64,
    ) -> Result<Arc<dyn acp_core::ports::SessionEndpoint>, PortError>;

    /// 按精确资源代释放临时端点。
    async fn release(&self, session: SessionId, generation: u64) -> EndpointRelease;

    /// 关闭序列：释放仍持有的全部临时端点（组合根在关闭 Agent 宿主**之前**调用）。
    ///
    /// 释放失败不返回值（关闭路径已经在收尾），但实现**不得**伪造成释放成功：失败的名字/数量由实现
    /// 自己记日志。它不接受参数，因此不会误关某个仍然活着的正式会话以外的东西。
    async fn release_all(&self);
}

// ---------------------------------------------------------------------------------------------
// 共享服务：凭据验证、committed 投影、运行期检查、签发 ready 凭据
// ---------------------------------------------------------------------------------------------

/// 凭据验证失败的对端可见分类（不泄露 claims 值或具体失败字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptReject {
    /// 当前连接/设备不再满足认证前提。
    Auth(AuthReject),
    /// 签名、编码或 claims 绑定不符（统一折叠，不泄露目标存在性）。
    Binding,
    /// 凭据的 storeEpoch 与当前持久值不一致：存储已整体重建。
    StoreRebuilt,
    /// 本机信任/密钥不可读（临时故障）。
    Unavailable,
}

/// 认证前提的具名失败（映射到 §8 的既有错误码与 close code）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthReject {
    /// 没有已认证设备身份。
    Required,
    /// 设备已撤销。
    Revoked,
    /// 设备未知。
    Unknown,
    /// 凭据缩减，必须重新认证。
    ReauthenticationRequired,
    /// 连接没有可比较的 canonical origin。
    OriginMismatch,
}

/// 已核验的凭据绑定（只由 [`Preparations::verify`] 构造）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// 凭据绑定的设备（必须等于当前连接）。
    pub device: DeviceId,
    /// Owner 分配的会话 ID。
    pub session: SessionId,
    /// 标签页归属。
    pub tab_nonce: Nonce,
    /// Agent 选择。
    pub agent: AgentId,
    /// 规范化工作目录路径。
    pub workspace_path: String,
    /// `true` = 路径来自已登记目录标识；`false` = 客户端绝对路径。
    pub from_directory: bool,
    /// 已验证的闭合 claims（重新签发时原样复用）。
    pub claims: PreparationClaims,
    /// claims transcript 的摘要（运行期暂存绑定用；不含正文）。
    pub claims_digest: Digest,
}

/// `committed` 分支的正式关联（D3.1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// 首次提交标识（不是会话 ID，也不是附件清单标识）。
    pub first_submission_id: FirstSubmissionId,
    /// 首个请求标识。
    pub first_request_id: Uuid,
    /// 当前尝试的请求标识。
    pub current_request_id: Uuid,
    /// 持久首条用户消息标识。
    pub first_message_id: Uuid,
}

/// 运行期检查的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // `Ready` 持有完整运行期视图；装箱只会增加 adapter 噪声。
pub enum RuntimeCheck {
    /// 资源代、租约与状态都可用于本次动作。
    Ready(PreparationView),
    /// 资源代已失效（`configure`/`renew`/`release` 与附件 begin/manifest 的 `stale` 来源）。
    Stale(PreparationStaleReason),
}

/// 准备面的共享服务：控制帧与附件目标共用同一份凭据验证与运行期判定。
pub struct Preparations {
    trust: Arc<dyn PreparationTrust>,
    directory: Arc<PreparationDirectory>,
    first_submission: Arc<dyn FirstSubmission>,
    staging: Option<Arc<dyn PreparationAttachmentStaging>>,
}

impl std::fmt::Debug for Preparations {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("sync::Preparations")
            .field("staging", &self.staging.is_some())
            .finish_non_exhaustive()
    }
}

impl Preparations {
    /// 装配。`staging` 为 `None` 表示本进程没有准备附件暂存端口（附件准备目标显式拒绝）。
    pub fn new(
        trust: Arc<dyn PreparationTrust>,
        directory: Arc<PreparationDirectory>,
        first_submission: Arc<dyn FirstSubmission>,
        staging: Option<Arc<dyn PreparationAttachmentStaging>>,
    ) -> Self {
        Self {
            trust,
            directory,
            first_submission,
            staging,
        }
    }

    /// 信任/签名面。
    pub fn trust(&self) -> &Arc<dyn PreparationTrust> {
        &self.trust
    }

    /// 运行期准备目录（本进程唯一）。
    pub fn directory(&self) -> &Arc<PreparationDirectory> {
        &self.directory
    }

    /// 准备附件暂存端口。
    pub fn staging(&self) -> Option<&Arc<dyn PreparationAttachmentStaging>> {
        self.staging.as_ref()
    }

    /// 当前时刻。
    pub fn now(&self) -> Timestamp {
        self.trust.now()
    }

    /// 该会话的正式受理投影（`restore`/`configure`/`renew`/`release` 的 committed 分支）。
    pub async fn committed(&self, session: &SessionId) -> Result<Option<Committed>, PortError> {
        let record = self
            .first_submission
            .first_submission(session.clone())
            .await?;
        Ok(record.and_then(committed_of))
    }

    /// 验证一份准备凭据：认证/撤销 → 签名 → claims 绑定。
    ///
    /// `session` 是本次请求指向的会话（`restore` 来自请求 envelope 的 `sessionId`，附件来自 target）。
    /// 验证只读取本机信任/密钥与 receipt 本身；**不**读准备目录、不建端点、不发生任何准备副作用。
    pub async fn verify(
        &self,
        device: &DeviceId,
        origin: Option<&CanonicalOrigin>,
        receipt: &sync_protocol::common::PreparationReceipt,
        session: &SessionId,
    ) -> Result<Verified, ReceiptReject> {
        let snapshot = self
            .trust
            .snapshot(device)
            .await
            .map_err(|_| ReceiptReject::Unavailable)?;
        match snapshot.credential {
            CredentialStatus::Active => {}
            CredentialStatus::Revoked => return Err(ReceiptReject::Auth(AuthReject::Revoked)),
            CredentialStatus::Unknown => return Err(ReceiptReject::Auth(AuthReject::Unknown)),
            CredentialStatus::ScopeReduced => {
                return Err(ReceiptReject::Auth(AuthReject::ReauthenticationRequired));
            }
        }
        let origin = origin.ok_or(ReceiptReject::Auth(AuthReject::OriginMismatch))?;
        let receipt = identity_receipt(receipt).ok_or(ReceiptReject::Binding)?;
        if receipt.claims().session_id != *session {
            return Err(ReceiptReject::Binding);
        }
        let public_key = self
            .trust
            .node_public_key()
            .await
            .map_err(|_| ReceiptReject::Unavailable)?;
        // 期望值必须来自**当前**持久事实，不能由 receipt 自证：owner/storeEpoch 取自刚读到的
        // 信任快照，否则旧存储代或别的节点的 receipt 会被当成有效。
        let expected = expectation_of(receipt.claims(), &snapshot);
        let authenticated = Authenticated {
            fact: IdentityFact::Device {
                device: device.clone(),
            },
            // 快照是刚读到的持久事实；receipt 不能替它决定。
            credential: snapshot.credential,
            connection: ConnectionBinding {
                canonical_origin: Some(origin.clone()),
                host: origin.authority().to_owned(),
                endpoint: None,
            },
        };
        verify_preparation_receipt(&authenticated, &public_key, &receipt, &expected).map_err(
            |error| match error {
                PreparationError::DeviceRevoked => ReceiptReject::Auth(AuthReject::Revoked),
                PreparationError::DeviceUnknown => ReceiptReject::Auth(AuthReject::Unknown),
                PreparationError::Unauthenticated => ReceiptReject::Auth(AuthReject::Required),
                PreparationError::ReauthenticationRequired => {
                    ReceiptReject::Auth(AuthReject::ReauthenticationRequired)
                }
                PreparationError::StoreRebuilt => ReceiptReject::StoreRebuilt,
                PreparationError::Keystore(_) => ReceiptReject::Unavailable,
                _ => ReceiptReject::Binding,
            },
        )?;
        let claims = receipt.claims().clone();
        let claims_digest = claims_digest(&claims).ok_or(ReceiptReject::Binding)?;
        let from_directory = claims.workspace_source == ReceiptWorkspaceSource::DirectoryId;
        Ok(Verified {
            device: claims.device_id.clone(),
            session: claims.session_id.clone(),
            tab_nonce: claims.tab_nonce.clone(),
            agent: claims.agent_id.clone(),
            workspace_path: claims.normalized_workspace_path.as_str().to_owned(),
            from_directory,
            claims,
            claims_digest,
        })
    }

    /// 运行期检查：当前 runtimeEpoch/generation/租约/状态。
    ///
    /// 顺序固定：先比 runtimeEpoch（旧进程的准备一律 `runtime_epoch_mismatch`），再找目录项
    /// （没有条目 = 已释放或从未在本运行期建立），再比 generation，再看租约与状态。`require_ready`
    /// 为真时还要求准备已完成（附件上传与 `configure` 需要真实端点）。
    pub fn runtime(
        &self,
        session: &SessionId,
        runtime_epoch: &RuntimeEpoch,
        generation: ResourceGeneration,
        require_ready: bool,
    ) -> RuntimeCheck {
        if runtime_epoch != self.directory.runtime_epoch() {
            return RuntimeCheck::Stale(PreparationStaleReason::RuntimeEpochMismatch);
        }
        let Some(view) = self.directory.view(session) else {
            return RuntimeCheck::Stale(PreparationStaleReason::Released);
        };
        if view.generation != generation {
            return RuntimeCheck::Stale(PreparationStaleReason::GenerationMismatch);
        }
        let expired = view
            .lease_expires_at
            .as_ref()
            .is_none_or(|expires| self.trust.now() >= *expires);
        if expired {
            return RuntimeCheck::Stale(PreparationStaleReason::Released);
        }
        if require_ready && view.state != PreparationState::Ready {
            // 资源代对得上但端点尚未确认：让客户端先 `restore`（拿到 ready 的资源代）再重试。
            return RuntimeCheck::Stale(PreparationStaleReason::GenerationMismatch);
        }
        RuntimeCheck::Ready(view)
    }

    /// 已核验 claims 与当前运行期条目的一致性（Agent/标签页/目录）。
    ///
    /// claims 是节点签发的，但仍必须与**当前**资源代指向的条目一致：否则一份旧 receipt 不能授权
    /// 另一份运行期资源。
    pub fn matches_runtime(&self, verified: &Verified, view: &PreparationView) -> bool {
        view.session == verified.session
            && view.owner.device == verified.device
            && view.owner.tab_nonce == verified.tab_nonce
            && view.agent.agent_id() == &verified.agent
            && derived_directory(&verified.workspace_path)
                .is_some_and(|directory| view.directory.as_ref() == Some(&directory))
    }

    /// 按当前资源代重新签发一份 ready 凭据（claims 与运行期条目逐项一致）。
    pub async fn receipt_for(
        &self,
        snapshot: &TrustSnapshot,
        origin: &CanonicalOrigin,
        view: &PreparationView,
        workspace_path: &str,
        from_directory: bool,
    ) -> Result<sync_protocol::common::PreparationReceipt, PortError> {
        let claims = PreparationClaims {
            device_id: view.owner.device.clone(),
            owner_node_id: snapshot.local_node.clone(),
            store_epoch: snapshot.server_epoch.clone(),
            origin: origin.clone(),
            tab_nonce: view.owner.tab_nonce.clone(),
            session_id: view.session.clone(),
            agent_id: view.agent.agent_id().clone(),
            normalized_workspace_path: NormalizedWorkspacePath::parse(workspace_path)
                .map_err(|_| PortError::InvalidRequest("workspace path is not canonical"))?,
            workspace_source: if from_directory {
                ReceiptWorkspaceSource::DirectoryId
            } else {
                ReceiptWorkspaceSource::WorkspacePath
            },
        };
        let receipt = self.trust.sign(&claims).await?;
        wire_receipt(&receipt).ok_or(PortError::Corrupt(
            "preparation receipt cannot be projected",
        ))
    }
}

/// 正式记录 → `committed` 投影。
fn committed_of(record: FirstSubmissionRecord) -> Option<Committed> {
    Some(Committed {
        first_submission_id: record.first_submission_id,
        first_request_id: Uuid::parse(record.first_request_id.as_str()).ok()?,
        current_request_id: Uuid::parse(record.current_request_id.as_str()).ok()?,
        first_message_id: Uuid::parse(record.user_message_id.as_str()).ok()?,
    })
}

/// `Committed` → 闭合结果分支。
fn committed_outcome(committed: &Committed) -> Option<PreparationOutcome> {
    Some(PreparationOutcome::Committed {
        first_submission_id: Uuid::parse(committed.first_submission_id.as_str()).ok()?,
        first_request_id: committed.first_request_id.clone(),
        current_request_id: committed.current_request_id.clone(),
        first_message_id: committed.first_message_id.clone(),
    })
}

/// 由规范化路径派生目录标识（与 core 的 `ResolvedWorkspace::at_path` 同源）。
fn derived_directory(path: &str) -> Option<acp_core::model::DirectoryId> {
    ResolvedWorkspace::at_path(path.to_owned())
        .ok()
        .map(|workspace| workspace.directory_id().clone())
}

/// claims 的 transcript 摘要（运行期暂存绑定；不含正文或秘密）。
fn claims_digest(claims: &PreparationClaims) -> Option<Digest> {
    use base64::Engine as _;
    use sha2::Digest as _;
    let transcript = claims.transcript().ok()?;
    let mut hasher = sha2::Sha256::new();
    hasher.update(transcript);
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
    Digest::new(&encoded).ok()
}

/// `PreparationExpectation` 与已验证 claims 一致（verify 已在内部逐项比较）。
fn expectation_of(claims: &PreparationClaims, snapshot: &TrustSnapshot) -> PreparationExpectation {
    PreparationExpectation {
        owner_node_id: snapshot.local_node.clone(),
        store_epoch: snapshot.server_epoch.clone(),
        session_id: claims.session_id.clone(),
        tab_nonce: claims.tab_nonce.clone(),
        agent_id: claims.agent_id.clone(),
        normalized_workspace_path: claims.normalized_workspace_path.clone(),
        workspace_source: claims.workspace_source,
    }
}

/// wire 的闭合 receipt → `identity-auth` 的强类型 receipt（结构已由 wire 校验）。
fn identity_receipt(
    receipt: &sync_protocol::common::PreparationReceipt,
) -> Option<PreparationReceipt> {
    let signature = P1363Signature::try_from_bytes(receipt.signature.as_bytes()).ok()?;
    Some(PreparationReceipt::new(
        identity_claims(&receipt.claims)?,
        signature,
    ))
}

/// wire claims → `identity-auth` claims。
fn identity_claims(
    claims: &sync_protocol::common::PreparationReceiptClaims,
) -> Option<PreparationClaims> {
    Some(PreparationClaims {
        device_id: DeviceId::new(claims.device_id.as_str()).ok()?,
        owner_node_id: NodeId::new(claims.owner_node_id.as_str()).ok()?,
        store_epoch: ServerEpoch::new(claims.store_epoch.as_str()).ok()?,
        origin: CanonicalOrigin::parse(claims.origin.as_str()).ok()?,
        tab_nonce: Nonce::new(&base64url_text(&claims.tab_nonce)).ok()?,
        session_id: SessionId::new(claims.session_id.as_str()).ok()?,
        agent_id: AgentId::new(claims.agent_id.as_str()).ok()?,
        normalized_workspace_path: NormalizedWorkspacePath::parse(
            claims.normalized_workspace_path.as_str(),
        )
        .ok()?,
        workspace_source: match claims.workspace_source {
            sync_protocol::common::PreparationWorkspaceSource::DirectoryId => {
                ReceiptWorkspaceSource::DirectoryId
            }
            sync_protocol::common::PreparationWorkspaceSource::WorkspacePath => {
                ReceiptWorkspaceSource::WorkspacePath
            }
        },
    })
}

/// 无填充 base64url 编码（wire 的 `Base64Url<32>` 没有文本访问器，而 core 的 `Nonce` 只接受文本）。
fn base64url_text<const BYTES: usize>(value: &Base64Url<BYTES>) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.as_bytes())
}

/// `identity-auth` receipt → wire receipt。
fn wire_receipt(receipt: &PreparationReceipt) -> Option<sync_protocol::common::PreparationReceipt> {
    let claims = receipt.claims();
    Some(sync_protocol::common::PreparationReceipt {
        claims: sync_protocol::common::PreparationReceiptClaims {
            version: ProtocolVersionV1::new(1).ok()?,
            owner_node_id: Uuid::parse(claims.owner_node_id.as_str()).ok()?,
            store_epoch: Uuid::parse(claims.store_epoch.as_str()).ok()?,
            origin: NonEmptyText::<255>::parse(claims.origin.as_str()).ok()?,
            device_id: Uuid::parse(claims.device_id.as_str()).ok()?,
            tab_nonce: Base64Url::<32>::parse(claims.tab_nonce.as_str()).ok()?,
            session_id: Uuid::parse(claims.session_id.as_str()).ok()?,
            agent_id: NonEmptyText::<128>::parse(claims.agent_id.as_str()).ok()?,
            normalized_workspace_path: NonEmptyText::<4096>::parse(
                claims.normalized_workspace_path.as_str(),
            )
            .ok()?,
            workspace_source: match claims.workspace_source {
                ReceiptWorkspaceSource::DirectoryId => {
                    sync_protocol::common::PreparationWorkspaceSource::DirectoryId
                }
                ReceiptWorkspaceSource::WorkspacePath => {
                    sync_protocol::common::PreparationWorkspaceSource::WorkspacePath
                }
            },
        },
        signature: Base64Url::<64>::parse(&receipt.signature_base64url()).ok()?,
    })
}

/// 把 `ModeState.available` 投影成 wire 的 `modeRef[]`。
fn wire_modes(modes: &ModeState) -> Option<Vec<sync_protocol::common::ModeRef>> {
    modes
        .available
        .iter()
        .map(crate::sync::command::query::wire_mode_ref)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// 帧级错误（尚未能构造闭合结果时）
// ---------------------------------------------------------------------------------------------

/// 帧级拒绝（`error` 消息；`correlationId` 是该请求帧的 `messageId`）。
#[derive(Debug, Clone)]
pub struct ControlError {
    code: ErrorCode,
    message: &'static str,
    details: RawObject,
    /// `Some` = 发完错误后按该 close code 结束连接（认证/撤销）。
    close: Option<(u16, &'static str)>,
}

impl ControlError {
    fn new(code: ErrorCode, message: &'static str, details: RawObject) -> Self {
        Self {
            code,
            message,
            details,
            close: None,
        }
    }

    fn closing(code: ErrorCode, message: &'static str, close: (u16, &'static str)) -> Self {
        Self {
            code,
            message,
            details: RawObject::empty(),
            close: Some(close),
        }
    }

    /// 拆成命令拒绝可用的三元组（`session.start` 的载荷校验与运行期检查共用）。
    pub(crate) fn into_parts(self) -> (ErrorCode, &'static str, RawObject) {
        (self.code, self.message, self.details)
    }
}

/// core 端口错误 → 帧级拒绝（复用 `sync::command` 的登记映射，不另立一套分类）。
fn port_control_error(error: &PortError) -> ControlError {
    let (code, message, details) = crate::sync::command::port_rejection(error).into_parts();
    ControlError::new(code, message, details)
}

/// 凭据拒绝 → 帧级拒绝。
fn receipt_control_error(reject: ReceiptReject) -> ControlError {
    match reject {
        ReceiptReject::Auth(AuthReject::Required) => ControlError::closing(
            ErrorCode::AuthRequired,
            "the connection has no authenticated device",
            (close::UNAUTHENTICATED, "authentication required"),
        ),
        ReceiptReject::Auth(AuthReject::Revoked) => ControlError::closing(
            ErrorCode::AuthDeviceRevoked,
            "the device is revoked",
            (close::REVOKED, "device revoked"),
        ),
        ReceiptReject::Auth(AuthReject::Unknown) => ControlError::closing(
            ErrorCode::AuthDeviceUnknown,
            "the device is not known to this host",
            (close::UNAUTHENTICATED, "device unknown"),
        ),
        ReceiptReject::Auth(AuthReject::ReauthenticationRequired) => ControlError::closing(
            ErrorCode::AuthRequired,
            "the device credential must be reauthenticated",
            (close::STATE_CONFLICT, "reauthentication required"),
        ),
        ReceiptReject::Auth(AuthReject::OriginMismatch) => ControlError::closing(
            ErrorCode::AuthOriginMismatch,
            "the connection origin does not match the device binding",
            (close::UNAUTHENTICATED, "origin mismatch"),
        ),
        ReceiptReject::Binding => ControlError::new(
            ErrorCode::AttachmentBindingMismatch,
            "the preparation receipt does not bind this resource",
            RawObject::empty(),
        ),
        ReceiptReject::StoreRebuilt => ControlError::new(
            ErrorCode::PreparationStoreRebuilt,
            "the persistent store was rebuilt; the preparation receipt is no longer valid",
            RawObject::empty(),
        ),
        ReceiptReject::Unavailable => ControlError::new(
            ErrorCode::InternalUnavailable,
            "the preparation receipt cannot be verified right now",
            RawObject::empty(),
        ),
    }
}

/// 凭据拒绝 → 命令拒绝的三元组（`session.start` 复用同一套错误码与文案，不另立分类）。
pub(crate) fn receipt_rejection(reject: ReceiptReject) -> (ErrorCode, &'static str, RawObject) {
    let error = receipt_control_error(reject);
    (error.code, error.message, error.details)
}

/// 运行期过代 → `preparation.stale`（`details.reason` 是唯一登记字段）。
pub(crate) fn stale_rejection(
    reason: PreparationStaleReason,
) -> (ErrorCode, &'static str, RawObject) {
    let details = RawObject::parse(&format!(r#"{{"reason":"{}"}}"#, reason.as_str()))
        .unwrap_or_else(|_| RawObject::empty());
    (
        ErrorCode::PreparationStale,
        "the preparation resource is not current any more",
        details,
    )
}

/// 闭合结果里的 `failed` 错误（retryable 取错误码的登记值）。
fn failed_outcome(
    code: ErrorCode,
    message: &'static str,
    details: RawObject,
) -> Option<PreparationOutcome> {
    Some(PreparationOutcome::Failed {
        error: sync_protocol::common::PublicError {
            code,
            message: Text::<1024>::parse(message).ok()?,
            retryable: code.default_retryable(),
            details,
        },
    })
}

fn wire_stale(reason: PreparationStaleReason) -> WireStaleReason {
    match reason {
        PreparationStaleReason::RuntimeEpochMismatch => WireStaleReason::RuntimeEpochMismatch,
        PreparationStaleReason::GenerationMismatch => WireStaleReason::GenerationMismatch,
        PreparationStaleReason::Released => WireStaleReason::Released,
    }
}

// ---------------------------------------------------------------------------------------------
// 控制帧路由
// ---------------------------------------------------------------------------------------------

/// 一条连接的控制面状态（协商结果 + 连接期去重表）。
#[derive(Debug, Default)]
struct ConnectionState {
    negotiated: bool,
    answered: Vec<DedupEntry>,
}

#[derive(Debug)]
struct DedupEntry {
    operation_id: Uuid,
    fingerprint: Vec<u8>,
    result: SessionPreparationResult,
}

/// 一条连接的准备控制帧计数窗口。
#[derive(Debug, Default)]
struct RateState {
    attempts: VecDeque<Instant>,
}

/// 按连接计数的准备控制帧限流器（键是连接：额度属于这条连接的页面）。
#[derive(Debug)]
struct PreparationRateLimiter {
    limit: u32,
    window: Duration,
    state: Mutex<BTreeMap<String, RateState>>,
}

impl PreparationRateLimiter {
    fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            state: Mutex::new(BTreeMap::new()),
        }
    }

    fn check_at(&self, connection: &str, now: Instant) -> Result<(), (Duration, bool)> {
        let mut state = lock(&self.state);
        if state.len() >= MAX_TRACKED_KEYS && !state.contains_key(connection) {
            // 键上限：失败关闭而不是重置既有计数。
            return Err((self.window, false));
        }
        let entry = state.entry(connection.to_owned()).or_default();
        while let Some(front) = entry.attempts.front() {
            if now.duration_since(*front) >= self.window {
                entry.attempts.pop_front();
            } else {
                break;
            }
        }
        if entry.attempts.len() >= self.limit as usize {
            let retry_after = entry
                .attempts
                .front()
                .map(|front| self.window.saturating_sub(now.duration_since(*front)))
                .unwrap_or(self.window);
            return Err((retry_after, false));
        }
        entry.attempts.push_back(now);
        Ok(())
    }

    fn forget(&self, connection: &str) {
        lock(&self.state).remove(connection);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `session.preparation` / `session.preparation.result` 的路由。
pub struct PreparationRoute {
    preparations: Arc<Preparations>,
    host: Arc<dyn PreparationEndpoints>,
    rate: PreparationRateLimiter,
    states: Mutex<BTreeMap<String, ConnectionState>>,
}

impl std::fmt::Debug for PreparationRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("sync::PreparationRoute")
            .field("connections", &lock(&self.states).len())
            .finish_non_exhaustive()
    }
}

impl PreparationRoute {
    /// 装配（组合根在端口齐备时注册；未注册时本 feature 不宣告）。
    pub fn new(preparations: Arc<Preparations>, host: Arc<dyn PreparationEndpoints>) -> Self {
        Self::with_rate_limit(
            preparations,
            host,
            PREPARATIONS_PER_MINUTE,
            PREPARATION_WINDOW,
        )
    }

    /// 显式限流参数（生产路径只用 [`PreparationRoute::new`]；用例用它把额度压小）。
    fn with_rate_limit(
        preparations: Arc<Preparations>,
        host: Arc<dyn PreparationEndpoints>,
        limit: u32,
        window: Duration,
    ) -> Self {
        Self {
            preparations,
            host,
            rate: PreparationRateLimiter::new(limit, window),
            states: Mutex::new(BTreeMap::new()),
        }
    }

    /// 用例构造：把控制帧额度压到 `limit` 次/分钟。
    #[cfg(test)]
    pub(crate) fn for_tests(
        preparations: Arc<Preparations>,
        host: Arc<dyn PreparationEndpoints>,
        limit: u32,
    ) -> Self {
        Self::with_rate_limit(preparations, host, limit, PREPARATION_WINDOW)
    }

    fn negotiated(&self, handle: &ConnectionHandle) -> bool {
        lock(&self.states)
            .get(handle.connection_id().as_str())
            .is_some_and(|state| state.negotiated)
    }

    fn send_frame_error(&self, handle: &ConnectionHandle, error: ControlError, correlation: &Uuid) {
        let mut body = match sync_protocol::error::Body::new(
            error.code,
            error.message,
            error.code.default_retryable(),
        ) {
            Ok(body) => body,
            Err(_) => return,
        };
        body.correlation_id = Nullable::value(correlation.clone());
        body.details = error.details;
        if let Err(fault) = handle.send(sync_protocol::envelope::MessageType::Error, &body) {
            debug!(
                event = "sync.preparation_error_not_queued",
                connection_id = handle.connection_id().as_str(),
                fault = ?fault,
                "the preparation error could not be queued"
            );
        }
        if let Some((code, reason)) = error.close {
            handle.request_close(code, reason);
        }
    }

    fn send_result(&self, handle: &ConnectionHandle, result: &SessionPreparationResult) -> bool {
        match handle.send(
            sync_protocol::envelope::MessageType::SessionPreparationResult,
            result,
        ) {
            Ok(()) => true,
            Err(fault) => {
                debug!(
                    event = "sync.preparation_result_not_queued",
                    connection_id = handle.connection_id().as_str(),
                    fault = ?fault,
                    "the preparation result could not be queued"
                );
                false
            }
        }
    }

    fn rate_limited(
        &self,
        handle: &ConnectionHandle,
        message: &sync_protocol::envelope::Envelope,
        retry_after: Duration,
    ) {
        debug!(
            event = "sync.preparation_rate_limited",
            connection_id = handle.connection_id().as_str(),
            retry_after_ms = retry_after.as_millis() as u64,
            "the connection exceeded its preparation control rate"
        );
        let details = RawObject::parse(&format!(
            r#"{{"retryAfterMs":{}}}"#,
            retry_after.as_millis() as u64
        ))
        .unwrap_or_else(|_| RawObject::empty());
        self.send_frame_error(
            handle,
            ControlError::new(
                ErrorCode::ResourceRateLimited,
                "the preparation control rate limit was exceeded",
                details,
            ),
            message.message_id(),
        );
    }

    async fn on_preparation(
        &self,
        handle: &ConnectionHandle,
        message: &sync_protocol::envelope::Envelope,
    ) -> RouteOutcome {
        // ① 认证前提（与业务命令同一把闸）：设备必须仍在本机持久信任表里且未撤销。
        let snapshot = match self.preparations.trust().snapshot(handle.device_id()).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                debug!(
                    event = "sync.preparation_device_failed",
                    connection_id = handle.connection_id().as_str(),
                    error = ?error,
                    "the persisted device snapshot cannot be read"
                );
                self.send_frame_error(
                    handle,
                    ControlError::new(
                        ErrorCode::InternalUnavailable,
                        "the device state cannot be read",
                        RawObject::empty(),
                    ),
                    message.message_id(),
                );
                return RouteOutcome::Claimed;
            }
        };
        match snapshot.credential {
            CredentialStatus::Active => {}
            CredentialStatus::Revoked => {
                self.send_frame_error(
                    handle,
                    receipt_control_error(ReceiptReject::Auth(AuthReject::Revoked)),
                    message.message_id(),
                );
                return RouteOutcome::Claimed;
            }
            CredentialStatus::Unknown => {
                self.send_frame_error(
                    handle,
                    receipt_control_error(ReceiptReject::Auth(AuthReject::Unknown)),
                    message.message_id(),
                );
                return RouteOutcome::Claimed;
            }
            CredentialStatus::ScopeReduced => {
                self.send_frame_error(
                    handle,
                    receipt_control_error(ReceiptReject::Auth(
                        AuthReject::ReauthenticationRequired,
                    )),
                    message.message_id(),
                );
                return RouteOutcome::Claimed;
            }
        }
        // ② feature 门控：未协商即显式拒绝，且不触碰准备目录/端点。
        if !self.negotiated(handle) {
            self.send_frame_error(
                handle,
                ControlError::new(
                    ErrorCode::ProtocolTypeUnsupported,
                    "the session preparation feature was not negotiated",
                    RawObject::empty(),
                ),
                message.message_id(),
            );
            return RouteOutcome::Claimed;
        }
        // ③ 连接期限流（只统计客户端 → 服务端的 session.preparation）。
        if let Err((retry_after, _)) = self
            .rate
            .check_at(handle.connection_id().as_str(), Instant::now())
        {
            self.rate_limited(handle, message, retry_after);
            return RouteOutcome::Claimed;
        }
        // ④ 形状（action 分支的字段出现性由 sync-protocol 的 DTO 钉死）。
        let request = match serde_json::from_str::<SessionPreparation>(message.body().get()) {
            Ok(request) => request,
            Err(_) => {
                self.send_frame_error(
                    handle,
                    ControlError::new(
                        ErrorCode::ProtocolSchemaInvalid,
                        "the session.preparation frame does not match the v1 schema",
                        RawObject::empty(),
                    ),
                    message.message_id(),
                );
                return RouteOutcome::Claimed;
            }
        };
        // ⑤ 连接期去重：同 operationId 同参数幂等回放；不同参数拒绝且不执行。
        let fingerprint = match serde_json::to_vec(&request) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.send_frame_error(
                    handle,
                    ControlError::new(
                        ErrorCode::InternalUnavailable,
                        "the preparation request cannot be fingerprinted",
                        RawObject::empty(),
                    ),
                    message.message_id(),
                );
                return RouteOutcome::Claimed;
            }
        };
        if let Some(previous) = self.answered(handle, &request.operation_id) {
            if previous.fingerprint == fingerprint {
                debug!(
                    event = "sync.preparation_duplicate",
                    connection_id = handle.connection_id().as_str(),
                    operation_id = request.operation_id.as_str(),
                    "the same preparation operation was already answered on this connection"
                );
                self.send_result(handle, &previous.result);
            } else {
                self.send_frame_error(
                    handle,
                    ControlError::new(
                        ErrorCode::CommandIdempotencyConflict,
                        "the operationId was already used for different preparation parameters",
                        RawObject::empty(),
                    ),
                    message.message_id(),
                );
            }
            return RouteOutcome::Claimed;
        }
        // ⑥ 执行动作。
        let outcome = self.dispatch(handle, &snapshot, &request).await;
        match outcome {
            Ok(result) => {
                self.remember(handle, &request.operation_id, fingerprint, result.clone());
                self.send_result(handle, &result);
            }
            Err(error) => self.send_frame_error(handle, error, message.message_id()),
        }
        RouteOutcome::Claimed
    }

    async fn dispatch(
        &self,
        handle: &ConnectionHandle,
        snapshot: &TrustSnapshot,
        request: &SessionPreparation,
    ) -> Result<SessionPreparationResult, ControlError> {
        match &request.detail {
            PreparationRequestDetail::Allocate {
                agent_id,
                workspace,
                tab_nonce,
            } => {
                self.allocate(handle, snapshot, request, agent_id, workspace, tab_nonce)
                    .await
            }
            PreparationRequestDetail::Restore { receipt, .. } => {
                self.restore(handle, snapshot, request, receipt).await
            }
            PreparationRequestDetail::Configure {
                receipt,
                runtime_epoch,
                generation,
                mode,
                config_options,
            } => {
                self.configure(
                    handle,
                    snapshot,
                    request,
                    receipt,
                    runtime_epoch,
                    generation,
                    mode,
                    config_options,
                )
                .await
            }
            PreparationRequestDetail::Renew {
                receipt,
                runtime_epoch,
                generation,
            } => {
                self.renew(
                    handle,
                    snapshot,
                    request,
                    receipt,
                    runtime_epoch,
                    generation,
                )
                .await
            }
            PreparationRequestDetail::Release {
                receipt,
                runtime_epoch,
                generation,
            } => {
                self.release(
                    handle,
                    snapshot,
                    request,
                    receipt,
                    runtime_epoch,
                    generation,
                )
                .await
            }
        }
    }

    fn answer(
        request: &SessionPreparation,
        session: &SessionId,
        outcome: PreparationOutcome,
    ) -> Option<SessionPreparationResult> {
        Some(SessionPreparationResult {
            operation_id: request.operation_id.clone(),
            action: request.action,
            session_id: Uuid::parse(session.as_str()).ok()?,
            outcome,
        })
    }

    fn failed_result(
        request: &SessionPreparation,
        session: &SessionId,
        code: ErrorCode,
        message: &'static str,
        details: RawObject,
    ) -> Result<SessionPreparationResult, ControlError> {
        Self::answer(
            request,
            session,
            failed_outcome(code, message, details).ok_or_else(|| {
                ControlError::new(
                    ErrorCode::InternalUnavailable,
                    "the preparation result cannot be projected",
                    RawObject::empty(),
                )
            })?,
        )
        .ok_or_else(|| {
            ControlError::new(
                ErrorCode::InternalUnavailable,
                "the preparation result cannot be projected",
                RawObject::empty(),
            )
        })
    }

    fn stale_result(
        request: &SessionPreparation,
        session: &SessionId,
        reason: PreparationStaleReason,
    ) -> Result<SessionPreparationResult, ControlError> {
        Self::answer(
            request,
            session,
            PreparationOutcome::Stale {
                reason: wire_stale(reason),
            },
        )
        .ok_or_else(|| {
            ControlError::new(
                ErrorCode::InternalUnavailable,
                "the preparation result cannot be projected",
                RawObject::empty(),
            )
        })
    }

    fn committed_result(
        request: &SessionPreparation,
        session: &SessionId,
        committed: &Committed,
    ) -> Result<SessionPreparationResult, ControlError> {
        Self::answer(
            request,
            session,
            committed_outcome(committed).ok_or_else(|| {
                ControlError::new(
                    ErrorCode::InternalUnavailable,
                    "the preparation result cannot be projected",
                    RawObject::empty(),
                )
            })?,
        )
        .ok_or_else(|| {
            ControlError::new(
                ErrorCode::InternalUnavailable,
                "the preparation result cannot be projected",
                RawObject::empty(),
            )
        })
    }

    fn ready_result(
        request: &SessionPreparation,
        view: &PreparationView,
        receipt: &sync_protocol::common::PreparationReceipt,
        options: &PreparationOptions,
    ) -> Result<SessionPreparationResult, ControlError> {
        Self::answer(
            request,
            &view.session,
            ready_outcome(receipt, view, options).ok_or_else(|| {
                ControlError::new(
                    ErrorCode::InternalUnavailable,
                    "the preparation result cannot be projected",
                    RawObject::empty(),
                )
            })?,
        )
        .ok_or_else(|| {
            ControlError::new(
                ErrorCode::InternalUnavailable,
                "the preparation result cannot be projected",
                RawObject::empty(),
            )
        })
    }

    async fn allocate(
        &self,
        handle: &ConnectionHandle,
        snapshot: &TrustSnapshot,
        request: &SessionPreparation,
        agent_id: &NonEmptyText<128>,
        workspace: &sync_protocol::sync::PreparationWorkspaceRef,
        tab_nonce: &Base64Url<32>,
    ) -> Result<SessionPreparationResult, ControlError> {
        let actor = Actor::Device {
            device: handle.device_id().clone(),
        };
        let source = match workspace {
            sync_protocol::sync::PreparationWorkspaceRef::Directory { directory_id } => {
                let directory =
                    acp_core::model::DirectoryId::new(directory_id.as_str()).map_err(|_| {
                        ControlError::new(
                            ErrorCode::ProtocolSchemaInvalid,
                            "the directoryId is not a directory id",
                            RawObject::empty(),
                        )
                    })?;
                PreparationWorkspaceSource::Directory(directory)
            }
            sync_protocol::sync::PreparationWorkspaceRef::Path { workspace_path } => {
                PreparationWorkspaceSource::Path(workspace_path.as_str().to_owned())
            }
        };
        let resolved = self
            .preparations
            .trust()
            .resolve_workspace(&actor, source)
            .await
            .map_err(|error| port_control_error(&error))?;
        let agent = AgentRef::try_new(
            AgentId::new(agent_id.as_str()).map_err(|_| {
                ControlError::new(
                    ErrorCode::ProtocolSchemaInvalid,
                    "the agentId is out of range",
                    RawObject::empty(),
                )
            })?,
            agent_id.as_str(),
        )
        .map_err(|_| {
            ControlError::new(
                ErrorCode::ProtocolSchemaInvalid,
                "the agentId is out of range",
                RawObject::empty(),
            )
        })?;
        let owner = PreparationOwner::new(
            handle.device_id().clone(),
            Nonce::new(&base64url_text(tab_nonce)).map_err(|_| {
                ControlError::new(
                    ErrorCode::ProtocolSchemaInvalid,
                    "the tabNonce is not a 32-byte nonce",
                    RawObject::empty(),
                )
            })?,
        );
        let now = self.preparations.now();
        let allocated = self
            .preparations
            .directory()
            .allocate(
                AllocatePreparation {
                    agent: agent.clone(),
                    directory: Some(resolved.workspace.directory_id().clone()),
                    owner,
                },
                now.clone(),
            )
            .await
            .map_err(|error| port_control_error(&error))?;
        let view = match allocated {
            AllocatePreparationOutcome::Allocated(view) => view,
            AllocatePreparationOutcome::CapacityExceeded => {
                return Err(ControlError::new(
                    ErrorCode::SessionCapacityExceeded,
                    "no active session slot is available right now",
                    RawObject::empty(),
                ));
            }
        };
        let options = self
            .host
            .establish(PreparationEstablish {
                session: view.session.clone(),
                agent,
                workspace: resolved.workspace.clone(),
                generation: view.generation.get(),
                selection: PreparationSelectionInput::default(),
            })
            .await;
        let options = match options {
            Ok(options) => options,
            Err(error) => {
                let _ = self.preparations.directory().mark_failed(
                    &view.session,
                    view.generation,
                    PreparationFailureReason::EndpointUnavailable,
                    now,
                );
                let (code, message, details) =
                    crate::sync::command::port_rejection(&error).into_parts();
                return Self::failed_result(request, &view.session, code, message, details);
            }
        };
        let decision =
            self.preparations
                .directory()
                .mark_ready(&view.session, view.generation, now);
        let view = ready_view(decision).ok_or_else(|| {
            ControlError::new(
                ErrorCode::InternalUnavailable,
                "the preparation resource vanished during allocation",
                RawObject::empty(),
            )
        })?;
        let origin = handle.canonical_origin().ok_or_else(|| {
            receipt_control_error(ReceiptReject::Auth(AuthReject::OriginMismatch))
        })?;
        let receipt = self
            .preparations
            .receipt_for(
                snapshot,
                origin,
                &view,
                resolved.workspace.canonical_path(),
                resolved.from_directory,
            )
            .await
            .map_err(|error| port_control_error(&error))?;
        Self::ready_result(request, &view, &receipt, &options)
    }

    /// 校验 receipt；storeEpoch 失效按 D3.1 返回 `failed` 结果，而不是认证/协议 error 帧。
    async fn verified_or_failed(
        &self,
        handle: &ConnectionHandle,
        request: &SessionPreparation,
        session: &SessionId,
        receipt: &sync_protocol::common::PreparationReceipt,
    ) -> Result<Result<Verified, SessionPreparationResult>, ControlError> {
        match self
            .preparations
            .verify(
                handle.device_id(),
                handle.canonical_origin(),
                receipt,
                session,
            )
            .await
        {
            Ok(verified) => Ok(Ok(verified)),
            Err(ReceiptReject::StoreRebuilt) => Ok(Err(Self::failed_result(
                request,
                session,
                ErrorCode::PreparationStoreRebuilt,
                "the persistent store was rebuilt; the preparation receipt is no longer valid",
                RawObject::empty(),
            )?)),
            Err(error) => Err(receipt_control_error(error)),
        }
    }

    async fn restore(
        &self,
        handle: &ConnectionHandle,
        snapshot: &TrustSnapshot,
        request: &SessionPreparation,
        receipt: &sync_protocol::common::PreparationReceipt,
    ) -> Result<SessionPreparationResult, ControlError> {
        let session = request_session(request)?;
        let verified = match self
            .verified_or_failed(handle, request, &session, receipt)
            .await?
        {
            Ok(verified) => verified,
            Err(result) => return Ok(result),
        };
        if let Some(committed) = self
            .preparations
            .committed(&session)
            .await
            .map_err(|error| port_control_error(&error))?
        {
            return Self::committed_result(request, &session, &committed);
        }
        // 从已核验 claims 重建同一 ID 的准备资源（有效 receipt 的 restore 永不返回 stale）。
        let workspace =
            ResolvedWorkspace::at_path(verified.workspace_path.clone()).map_err(|_| {
                ControlError::new(
                    ErrorCode::AttachmentBindingMismatch,
                    "the preparation receipt workspace is not usable",
                    RawObject::empty(),
                )
            })?;
        let agent =
            AgentRef::try_new(verified.agent.clone(), verified.agent.as_str()).map_err(|_| {
                ControlError::new(
                    ErrorCode::AttachmentBindingMismatch,
                    "the preparation receipt agent is not usable",
                    RawObject::empty(),
                )
            })?;
        let owner = PreparationOwner::new(verified.device.clone(), verified.tab_nonce.clone());
        let now = self.preparations.now();
        let allocated = self
            .preparations
            .directory()
            .restore_or_recreate(
                &session,
                AllocatePreparation {
                    agent: agent.clone(),
                    directory: Some(workspace.directory_id().clone()),
                    owner,
                },
                now.clone(),
            )
            .await
            .map_err(|error| port_control_error(&error))?;
        let view = match allocated {
            AllocatePreparationOutcome::Allocated(view) => view,
            AllocatePreparationOutcome::CapacityExceeded => {
                return Self::failed_result(
                    request,
                    &session,
                    ErrorCode::SessionCapacityExceeded,
                    "no active session slot is available right now",
                    RawObject::empty(),
                );
            }
        };
        let options = self
            .host
            .establish(PreparationEstablish {
                session: view.session.clone(),
                agent,
                workspace,
                generation: view.generation.get(),
                selection: PreparationSelectionInput::default(),
            })
            .await;
        let options = match options {
            Ok(options) => options,
            Err(error) => {
                let _ = self.preparations.directory().mark_failed(
                    &view.session,
                    view.generation,
                    PreparationFailureReason::EndpointUnavailable,
                    now,
                );
                let (code, message, details) =
                    crate::sync::command::port_rejection(&error).into_parts();
                return Self::failed_result(request, &view.session, code, message, details);
            }
        };
        let decision =
            self.preparations
                .directory()
                .mark_ready(&view.session, view.generation, now);
        let view = ready_view(decision).ok_or_else(|| {
            ControlError::new(
                ErrorCode::InternalUnavailable,
                "the preparation resource vanished during restore",
                RawObject::empty(),
            )
        })?;
        let origin = handle.canonical_origin().ok_or_else(|| {
            receipt_control_error(ReceiptReject::Auth(AuthReject::OriginMismatch))
        })?;
        let receipt = self
            .preparations
            .receipt_for(
                snapshot,
                origin,
                &view,
                verified.workspace_path.as_str(),
                verified.from_directory,
            )
            .await
            .map_err(|error| port_control_error(&error))?;
        Self::ready_result(request, &view, &receipt, &options)
    }

    #[allow(clippy::too_many_arguments)]
    async fn configure(
        &self,
        handle: &ConnectionHandle,
        snapshot: &TrustSnapshot,
        request: &SessionPreparation,
        receipt: &sync_protocol::common::PreparationReceipt,
        runtime_epoch: &Uuid,
        generation: &sync_protocol::common::PositiveDecimalString,
        mode: &Option<NonEmptyText<256>>,
        config_options: &Option<sync_protocol::command::PreparationConfigSelection>,
    ) -> Result<SessionPreparationResult, ControlError> {
        let session = request_session(request)?;
        let verified = match self
            .verified_or_failed(handle, request, &session, receipt)
            .await?
        {
            Ok(verified) => verified,
            Err(result) => return Ok(result),
        };
        if let Some(committed) = self
            .preparations
            .committed(&session)
            .await
            .map_err(|error| port_control_error(&error))?
        {
            return Self::committed_result(request, &session, &committed);
        }
        let target_epoch = parse_epoch(runtime_epoch)?;
        let target_generation = parse_generation(generation)?;
        let view = match self
            .preparations
            .runtime(&session, &target_epoch, target_generation, true)
        {
            RuntimeCheck::Ready(view) => {
                if !self.preparations.matches_runtime(&verified, &view) {
                    return Self::failed_result(
                        request,
                        &session,
                        ErrorCode::AttachmentBindingMismatch,
                        "the preparation receipt does not bind this resource",
                        RawObject::empty(),
                    );
                }
                view
            }
            RuntimeCheck::Stale(reason) => return Self::stale_result(request, &session, reason),
        };
        let selection = selection_input(mode, config_options)?;
        let now = self.preparations.now();
        let options = self
            .host
            .configure(PreparationConfigure {
                session: view.session.clone(),
                generation: view.generation.get(),
                selection,
            })
            .await;
        let options = match options {
            Ok(options) => options,
            Err(error) => {
                let _ = self.preparations.directory().mark_failed(
                    &view.session,
                    view.generation,
                    PreparationFailureReason::ConfigurationRejected,
                    now,
                );
                let (code, message, details) =
                    crate::sync::command::port_rejection(&error).into_parts();
                return Self::failed_result(request, &view.session, code, message, details);
            }
        };
        let decision =
            self.preparations
                .directory()
                .mark_ready(&view.session, view.generation, now);
        let view = ready_view(decision).ok_or_else(|| {
            ControlError::new(
                ErrorCode::InternalUnavailable,
                "the preparation resource vanished during configure",
                RawObject::empty(),
            )
        })?;
        let origin = handle.canonical_origin().ok_or_else(|| {
            receipt_control_error(ReceiptReject::Auth(AuthReject::OriginMismatch))
        })?;
        let receipt = self
            .preparations
            .receipt_for(
                snapshot,
                origin,
                &view,
                verified.workspace_path.as_str(),
                verified.from_directory,
            )
            .await
            .map_err(|error| port_control_error(&error))?;
        Self::ready_result(request, &view, &receipt, &options)
    }

    async fn renew(
        &self,
        handle: &ConnectionHandle,
        snapshot: &TrustSnapshot,
        request: &SessionPreparation,
        receipt: &sync_protocol::common::PreparationReceipt,
        runtime_epoch: &Uuid,
        generation: &sync_protocol::common::PositiveDecimalString,
    ) -> Result<SessionPreparationResult, ControlError> {
        let session = request_session(request)?;
        let verified = match self
            .verified_or_failed(handle, request, &session, receipt)
            .await?
        {
            Ok(verified) => verified,
            Err(result) => return Ok(result),
        };
        if let Some(committed) = self
            .preparations
            .committed(&session)
            .await
            .map_err(|error| port_control_error(&error))?
        {
            return Self::committed_result(request, &session, &committed);
        }
        let target_epoch = parse_epoch(runtime_epoch)?;
        let target_generation = parse_generation(generation)?;
        if let RuntimeCheck::Ready(view) =
            self.preparations
                .runtime(&session, &target_epoch, target_generation, false)
        {
            if !self.preparations.matches_runtime(&verified, &view) {
                return Self::failed_result(
                    request,
                    &session,
                    ErrorCode::AttachmentBindingMismatch,
                    "the preparation receipt does not bind this resource",
                    RawObject::empty(),
                );
            }
        }
        let decision = self.preparations.directory().renew(
            &session,
            target_generation,
            self.preparations.now(),
        );
        let view = match decision {
            PreparationDecision::Ready(view) => view,
            PreparationDecision::Stale(reason) => {
                return Self::stale_result(request, &session, reason);
            }
            PreparationDecision::Released | PreparationDecision::Unknown => {
                return Self::stale_result(request, &session, PreparationStaleReason::Released);
            }
        };
        // ready 结果必须带 Agent 确认过的真实选项：这里只读，不应用任何选择。
        let options = self
            .host
            .configure(PreparationConfigure {
                session: view.session.clone(),
                generation: view.generation.get(),
                selection: PreparationSelectionInput::default(),
            })
            .await;
        let options = match options {
            Ok(options) => options,
            Err(error) => {
                let (code, message, details) =
                    crate::sync::command::port_rejection(&error).into_parts();
                return Self::failed_result(request, &view.session, code, message, details);
            }
        };
        let origin = handle.canonical_origin().ok_or_else(|| {
            receipt_control_error(ReceiptReject::Auth(AuthReject::OriginMismatch))
        })?;
        let receipt = self
            .preparations
            .receipt_for(
                snapshot,
                origin,
                &view,
                verified.workspace_path.as_str(),
                verified.from_directory,
            )
            .await
            .map_err(|error| port_control_error(&error))?;
        Self::ready_result(request, &view, &receipt, &options)
    }

    async fn release(
        &self,
        handle: &ConnectionHandle,
        _snapshot: &TrustSnapshot,
        request: &SessionPreparation,
        receipt: &sync_protocol::common::PreparationReceipt,
        runtime_epoch: &Uuid,
        generation: &sync_protocol::common::PositiveDecimalString,
    ) -> Result<SessionPreparationResult, ControlError> {
        let session = request_session(request)?;
        let verified = match self
            .verified_or_failed(handle, request, &session, receipt)
            .await?
        {
            Ok(verified) => verified,
            Err(result) => return Ok(result),
        };
        if let Some(committed) = self
            .preparations
            .committed(&session)
            .await
            .map_err(|error| port_control_error(&error))?
        {
            return Self::committed_result(request, &session, &committed);
        }
        let target_epoch = parse_epoch(runtime_epoch)?;
        let target_generation = parse_generation(generation)?;
        if target_epoch != *self.preparations.directory().runtime_epoch() {
            return Self::stale_result(
                request,
                &session,
                PreparationStaleReason::RuntimeEpochMismatch,
            );
        }
        let Some(view) = self.preparations.directory().view(&session) else {
            // 同一运行期内已释放：幂等成功（不宣告 ID 已正式提交）。
            return Self::answer(request, &session, PreparationOutcome::Released).ok_or_else(
                || {
                    ControlError::new(
                        ErrorCode::InternalUnavailable,
                        "the preparation result cannot be projected",
                        RawObject::empty(),
                    )
                },
            );
        };
        if view.generation != target_generation {
            return Self::stale_result(
                request,
                &session,
                PreparationStaleReason::GenerationMismatch,
            );
        }
        if !self.preparations.matches_runtime(&verified, &view) {
            return Self::failed_result(
                request,
                &session,
                ErrorCode::AttachmentBindingMismatch,
                "the preparation receipt does not bind this resource",
                RawObject::empty(),
            );
        }
        let endpoint = self
            .host
            .release(session.clone(), target_generation.get())
            .await;
        if endpoint != EndpointRelease::Released {
            return Self::failed_result(
                request,
                &session,
                ErrorCode::InternalUnavailable,
                "the preparation endpoint could not be confirmed released",
                RawObject::empty(),
            );
        }
        let outcome = self
            .preparations
            .directory()
            .release(&session, target_generation, self.preparations.now())
            .await;
        if outcome != PreparationReleaseOutcome::Released {
            return Self::failed_result(
                request,
                &session,
                ErrorCode::InternalUnavailable,
                "the preparation capacity release is uncertain",
                RawObject::empty(),
            );
        }
        Self::answer(request, &session, PreparationOutcome::Released).ok_or_else(|| {
            ControlError::new(
                ErrorCode::InternalUnavailable,
                "the preparation result cannot be projected",
                RawObject::empty(),
            )
        })
    }

    fn answered(&self, handle: &ConnectionHandle, operation: &Uuid) -> Option<DedupEntry> {
        let states = lock(&self.states);
        states
            .get(handle.connection_id().as_str())
            .and_then(|state| {
                state
                    .answered
                    .iter()
                    .find(|entry| &entry.operation_id == operation)
            })
            .map(|entry| DedupEntry {
                operation_id: entry.operation_id.clone(),
                fingerprint: entry.fingerprint.clone(),
                result: entry.result.clone(),
            })
    }

    fn remember(
        &self,
        handle: &ConnectionHandle,
        operation: &Uuid,
        fingerprint: Vec<u8>,
        result: SessionPreparationResult,
    ) {
        let mut states = lock(&self.states);
        let state = states
            .entry(handle.connection_id().as_str().to_owned())
            .or_default();
        while state.answered.len() >= MAX_DEDUP_ENTRIES {
            state.answered.remove(0);
        }
        state.answered.push(DedupEntry {
            operation_id: operation.clone(),
            fingerprint,
            result,
        });
    }
}

#[async_trait::async_trait]
impl MessageRoute for PreparationRoute {
    async fn route(
        &self,
        session: &ConnectionHandle,
        message: &sync_protocol::envelope::Envelope,
    ) -> RouteOutcome {
        match message.message_type() {
            sync_protocol::envelope::MessageType::SessionPreparation => {
                self.on_preparation(session, message).await
            }
            // `session.preparation.result` 是服务端 → 客户端的方向：客户端发来时不认领，由连接面按
            // `protocol.type_unsupported` 拒绝（合法 v1 类型，只是方向不对）。
            _ => RouteOutcome::Unclaimed,
        }
    }

    fn on_authenticated(&self, session: &ConnectionHandle, features: &[String]) {
        let negotiated = features
            .iter()
            .any(|feature| feature == PREPARATION_FEATURE);
        lock(&self.states)
            .entry(session.connection_id().as_str().to_owned())
            .or_default()
            .negotiated = negotiated;
    }

    async fn on_closed(&self, session: &ConnectionHandle) {
        lock(&self.states).remove(session.connection_id().as_str());
        self.rate.forget(session.connection_id().as_str());
    }
}

// ---------------------------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------------------------

fn request_session(request: &SessionPreparation) -> Result<SessionId, ControlError> {
    request
        .session_id
        .as_ref()
        .and_then(|id| SessionId::new(id.as_str()).ok())
        .ok_or_else(|| {
            ControlError::new(
                ErrorCode::ProtocolSchemaInvalid,
                "the preparation request needs a sessionId",
                RawObject::empty(),
            )
        })
}

fn ready_view(decision: PreparationDecision) -> Option<PreparationView> {
    match decision {
        PreparationDecision::Ready(view) => Some(view),
        _ => None,
    }
}

fn ready_outcome(
    receipt: &sync_protocol::common::PreparationReceipt,
    view: &PreparationView,
    options: &PreparationOptions,
) -> Option<PreparationOutcome> {
    let lease = view.lease_expires_at.as_ref()?;
    Some(PreparationOutcome::Ready {
        receipt: receipt.clone(),
        runtime_epoch: Uuid::parse(view.runtime_epoch.as_str()).ok()?,
        generation: sync_protocol::common::PositiveDecimalString::parse(
            &view.generation.get().to_string(),
        )
        .ok()?,
        modes: wire_modes(&options.modes)?,
        config_options: options
            .config
            .iter()
            .map(crate::session_read::wire_config_option)
            .collect::<Option<Vec<_>>>()?,
        lease_expires_at: WireTimestamp::parse(lease.as_str()).ok()?,
    })
}

pub(crate) fn parse_epoch(value: &Uuid) -> Result<RuntimeEpoch, ControlError> {
    RuntimeEpoch::new(value.as_str()).map_err(|_| {
        ControlError::new(
            ErrorCode::ProtocolSchemaInvalid,
            "the runtimeEpoch is not a canonical UUID",
            RawObject::empty(),
        )
    })
}

pub(crate) fn parse_generation(
    value: &sync_protocol::common::PositiveDecimalString,
) -> Result<ResourceGeneration, ControlError> {
    value
        .as_str()
        .parse::<u64>()
        .map(ResourceGeneration::new)
        .map_err(|_| {
            ControlError::new(
                ErrorCode::ProtocolSchemaInvalid,
                "the generation is out of range",
                RawObject::empty(),
            )
        })
}

pub(crate) fn selection_input(
    mode: &Option<NonEmptyText<256>>,
    config_options: &Option<sync_protocol::command::PreparationConfigSelection>,
) -> Result<PreparationSelectionInput, ControlError> {
    let mode = match mode {
        Some(mode) => Some(ModeId::new(mode.as_str()).map_err(|_| {
            ControlError::new(
                ErrorCode::ProtocolSchemaInvalid,
                "the mode is out of range",
                RawObject::empty(),
            )
        })?),
        None => None,
    };
    let mut config = Vec::new();
    if let Some(selection) = config_options {
        for (id, value) in selection.as_map() {
            let id = ConfigOptionId::new(id).map_err(|_| {
                ControlError::new(
                    ErrorCode::ProtocolSchemaInvalid,
                    "a config option id is out of range",
                    RawObject::empty(),
                )
            })?;
            let value = match value {
                WireConfigValue::Boolean(flag) => ConfigValue::boolean(*flag),
                WireConfigValue::Text(text) => {
                    ConfigValue::select(text.as_str()).map_err(|_| {
                        ControlError::new(
                            ErrorCode::ProtocolSchemaInvalid,
                            "a config option value is out of range",
                            RawObject::empty(),
                        )
                    })?
                }
            };
            config.push((id, value));
        }
    }
    Ok(PreparationSelectionInput { mode, config })
}

#[cfg(test)]
#[path = "preparation_tests.rs"]
pub(crate) mod tests;
