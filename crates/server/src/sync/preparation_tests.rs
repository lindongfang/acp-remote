//! 准备控制面的协议级用例：认证优先、feature 门控、连接期去重、闭合结果分支、限流与凭据绑定。
//!
//! 全部依赖都是窄替身（[`PreparationTrust`]/[`FirstSubmission`]/[`PreparationAttachmentStaging`]/
//! [`PreparationEndpoints`]）：不装配完整 `UseCases`，也不触碰文件系统或真实 Agent。签名用一把固定的
//! P-256 测试私钥（`p256` 只在 dev-dependencies 里），因此「篡改/跨设备」负例走的是真实验签路径。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use acp_core::model::{
    AgentId, AgentRef, AttachmentId, AttachmentSubject, ClaimedAttachment, ConfigOption,
    ConnectionId, DeviceId, Digest, DispatchAdmission, DispatchFence, FirstSubmissionAdmission,
    FirstSubmissionConvergence, FirstSubmissionId, FirstSubmissionRecord,
    FirstSubmissionSettlement, FirstSubmissionWrite, FirstUserMessage, ModeRef, ModeState, NodeId,
    PortError, PreparationBinding, PreparationOwner, PreparationView, ResourceGeneration,
    RuntimeEpoch, SealedFacts, SessionId, StagedAbortReason, StagedChunk, StagedChunkOutcome,
    StagedClaim, StagedManifestBegin, StagedManifestPage, StagedManifestSeal, StagedManifestState,
    StagedSeal, StagedSealOutcome, StagedTransferBegin, StagedTransferState, SubmissionId,
    Timestamp, UnavailableKind, ViewJson,
};
use acp_core::ports::{
    Admission, CapacityAdmission, FirstSubmission, PreparationAttachmentStaging, ReclaimCandidate,
};
use async_trait::async_trait;
use identity_auth::{CanonicalOrigin, CredentialStatus, P1363Signature, PreparationReceipt};
use p256::ecdsa::signature::Signer as _;
use sync_protocol::common::{Base64Url, DecimalString, NonEmptyText, PositiveDecimalString, Uuid};
use sync_protocol::envelope::{ConnectionFields, Envelope, MessageType, encode_body};

use super::*;
use crate::sync::conn::limits::{SessionLimits, SyncConfig};
use crate::sync::conn::registry::Outbound;

pub(crate) const DEVICE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
pub(crate) const OTHER_DEVICE: &str = "9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a";
pub(crate) const NODE: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
pub(crate) const EPOCH: &str = "9c6a1e34-5b7d-4f2a-9d18-6e0b2c3d4e5f";
pub(crate) const REBUILT_EPOCH: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
pub(crate) const RUNTIME: &str = "00384a03-bc90-4095-b65d-82fb8cc47e13";
pub(crate) const ORIGIN: &str = "https://work-pc.example.ts.net";
pub(crate) const TAB: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
pub(crate) const AGENT: &str = "codex";
pub(crate) const AT: &str = "2026-01-01T00:00:00.000Z";

fn now() -> Timestamp {
    Timestamp::new(AT).expect("timestamp")
}

fn uuid(text: &str) -> Uuid {
    Uuid::parse(text).expect("uuid")
}

fn fresh() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn nonce(text: &str) -> Nonce {
    Nonce::new(text).expect("nonce")
}

pub(crate) fn canonical_origin() -> CanonicalOrigin {
    CanonicalOrigin::parse(ORIGIN).expect("canonical origin")
}

/// 固定测试私钥（绝不用于生产）。
fn signing_key() -> p256::ecdsa::SigningKey {
    p256::ecdsa::SigningKey::from_slice(&[7u8; 32]).expect("fixed scalar")
}

// ---------------------------------------------------------------------------------------------
// 窄替身
// ---------------------------------------------------------------------------------------------

pub(crate) struct TestIds;

impl acp_core::ports::IdGenerator for TestIds {
    fn session_id(&self) -> SessionId {
        SessionId::new(&fresh()).expect("session id")
    }
    fn turn_id(&self) -> acp_core::model::TurnId {
        acp_core::model::TurnId::new(&fresh()).expect("turn id")
    }
    fn interaction_id(&self) -> acp_core::model::InteractionId {
        acp_core::model::InteractionId::new(&fresh()).expect("interaction id")
    }
    fn pairing_id(&self) -> acp_core::model::PairingId {
        acp_core::model::PairingId::new(&fresh()).expect("pairing id")
    }
    fn message_id(&self) -> acp_core::model::MessageId {
        acp_core::model::MessageId::new(&fresh()).expect("message id")
    }
    fn origin_epoch(&self) -> acp_core::model::OriginEpoch {
        acp_core::model::OriginEpoch::new(&fresh()).expect("origin epoch")
    }
    fn request_id(&self) -> acp_core::model::RequestId {
        acp_core::model::RequestId::new(&fresh()).expect("request id")
    }
}

pub(crate) struct TestCapacity {
    full: Mutex<bool>,
    reserves: Mutex<Vec<String>>,
    releases: Mutex<Vec<(String, u64)>>,
}

impl TestCapacity {
    /// 成功预占的次数（`session.start` 的资源转移必须**不**新增一次）。
    pub(crate) fn reserve_calls(&self) -> usize {
        self.reserves.lock().expect("lock").len()
    }

    /// 已交还的名额（准备端点释放或正式会话回收）；转移给正式会话不算交还。
    pub(crate) fn release_calls(&self) -> usize {
        self.releases.lock().expect("lock").len()
    }
}

#[async_trait]
impl CapacityAdmission for TestCapacity {
    async fn reserve(&self, session: SessionId, _now: Timestamp) -> Result<Admission, PortError> {
        if *self.full.lock().expect("lock") {
            Ok(Admission::CapacityExceeded)
        } else {
            self.reserves
                .lock()
                .expect("lock")
                .push(session.as_str().to_owned());
            Ok(Admission::Reserved)
        }
    }
    async fn release(
        &self,
        session: SessionId,
        generation: ResourceGeneration,
        _now: Timestamp,
    ) -> Result<(), PortError> {
        self.releases
            .lock()
            .expect("lock")
            .push((session.as_str().to_owned(), generation.get()));
        Ok(())
    }
    async fn reclaim_candidate(
        &self,
        _now: Timestamp,
    ) -> Result<Option<ReclaimCandidate>, PortError> {
        Ok(None)
    }
}

pub(crate) struct TestTrust {
    node: NodeId,
    epoch: ServerEpoch,
    credential: Mutex<CredentialStatus>,
    workspace: String,
    signing: p256::ecdsa::SigningKey,
    public_key: PeerPublicKey,
}

impl TestTrust {
    fn new(workspace: &str) -> Self {
        let signing = signing_key();
        let verifying = p256::ecdsa::VerifyingKey::from(&signing);
        let point = verifying.to_sec1_point(false);
        let public_key = PeerPublicKey::try_from_bytes(point.as_bytes()).expect("public key");
        Self {
            node: NodeId::new(NODE).expect("node"),
            epoch: ServerEpoch::new(EPOCH).expect("epoch"),
            credential: Mutex::new(CredentialStatus::Active),
            workspace: workspace.to_owned(),
            signing,
            public_key,
        }
    }

    pub(crate) fn workspace(&self) -> &str {
        &self.workspace
    }

    /// 同步签一份 claims（用例直接构造 wire receipt，避免在异步测试里再起运行时）。
    pub(crate) fn sign_sync(&self, claims: &PreparationClaims) -> PreparationReceipt {
        let transcript = claims.transcript().expect("claims assemble");
        let signature: p256::ecdsa::Signature = self.signing.sign(&transcript);
        let signature =
            P1363Signature::try_from_bytes(signature.to_bytes().as_slice()).expect("signature");
        PreparationReceipt::new(claims.clone(), signature)
    }

    fn set_credential(&self, credential: CredentialStatus) {
        *self.credential.lock().expect("lock") = credential;
    }
}

#[async_trait]
impl PreparationTrust for TestTrust {
    async fn snapshot(&self, device: &DeviceId) -> Result<TrustSnapshot, PortError> {
        // `OTHER_DEVICE` 也是已认证设备：跨设备用例应走到 receipt 绑定校验，而不是提前以
        // `auth.device_unknown` 结束；撤销/未知仍由 `credential` 的显式取值覆盖。
        let credential = if device.as_str() == DEVICE || device.as_str() == OTHER_DEVICE {
            *self.credential.lock().expect("lock")
        } else {
            CredentialStatus::Unknown
        };
        Ok(TrustSnapshot {
            local_node: self.node.clone(),
            server_epoch: self.epoch.clone(),
            credential,
        })
    }

    async fn node_public_key(&self) -> Result<PeerPublicKey, PortError> {
        Ok(self.public_key.clone())
    }

    async fn sign(&self, claims: &PreparationClaims) -> Result<PreparationReceipt, PortError> {
        Ok(self.sign_sync(claims))
    }

    async fn resolve_workspace(
        &self,
        _actor: &Actor,
        source: PreparationWorkspaceSource,
    ) -> Result<PreparationWorkspace, PortError> {
        let (text, from_directory) = match source {
            PreparationWorkspaceSource::Directory(_) => (self.workspace.clone(), true),
            PreparationWorkspaceSource::Path(path) => (path, false),
        };
        Ok(PreparationWorkspace {
            workspace: ResolvedWorkspace::at_path(text)
                .map_err(|_| PortError::InvalidRequest("workspace"))?,
            from_directory,
        })
    }

    fn now(&self) -> Timestamp {
        now()
    }
}

/// 内存首条存储的可观察状态：`session.start` 的「提交过一次」「派发过一次」「只存一份首条」都由它回答。
#[derive(Default)]
pub(crate) struct FirstState {
    /// 会话 → 首条关联（受理即写入，派发/结算只条件更新）。
    pub(crate) records: BTreeMap<String, FirstSubmissionRecord>,
    /// 会话 → 持久首条用户消息（继续的正文来源）。
    pub(crate) messages: BTreeMap<String, FirstUserMessage>,
    /// 持久命令行（`find_request` 的幂等事实；受理与派发围栏在同一事务写）。
    pub(crate) commands: Vec<acp_core::model::CommandRecord>,
    /// 受理事务次数（重复请求必须不再增加）。
    pub(crate) commits: u32,
    /// 跨越派发围栏的次数（阶段 CAS 只允许一次）。
    pub(crate) dispatches: u32,
    /// 结算写次数（继续的准备阶段失败）。
    pub(crate) settlements: u32,
    /// 下一次受理事务注入的失败（`None` = 正常受理）。
    pub(crate) fail_commit: Option<UnavailableKind>,
}

pub(crate) struct TestFirst {
    state: Arc<Mutex<FirstState>>,
}

impl Default for TestFirst {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(FirstState::default())),
        }
    }
}

impl TestFirst {
    /// 预置一条首条关联（等价于「存储里已有这一行」）；后续的派发/结算会条件更新同一行。
    pub(crate) fn set(&self, record: FirstSubmissionRecord) {
        self.state
            .lock()
            .expect("lock")
            .records
            .insert(record.session.as_str().to_owned(), record);
    }

    /// 共享状态（同一个实例同时接在 `Preparations` 与 `UseCases`/broker 上，与组合根一致）。
    pub(crate) fn state(&self) -> &Arc<Mutex<FirstState>> {
        &self.state
    }

    pub(crate) fn commits(&self) -> u32 {
        self.state.lock().expect("lock").commits
    }

    pub(crate) fn dispatches(&self) -> u32 {
        self.state.lock().expect("lock").dispatches
    }

    pub(crate) fn settlements(&self) -> u32 {
        self.state.lock().expect("lock").settlements
    }

    /// 注入一次受理事务失败（原子保存失败 MUST NOT 派发）。
    pub(crate) fn fail_next_commit(&self, kind: UnavailableKind) {
        self.state.lock().expect("lock").fail_commit = Some(kind);
    }

    fn record_command(write: &FirstSubmissionWrite) -> acp_core::model::CommandRecord {
        acp_core::model::CommandRecord::try_new(
            Some(write.session.clone()),
            write.request_id.clone(),
            "session.start",
            acp_core::model::CommandKind::Mutation,
            write.actor.clone(),
            Some(write.at.clone()),
            acp_core::model::CommandStatus::Accepted,
            None,
            None,
            None,
            None,
            None,
            write.request_fingerprint.clone(),
        )
        .expect("accepted command row")
    }
}

#[async_trait]
impl FirstSubmission for TestFirst {
    async fn commit_first_submission(
        &self,
        write: FirstSubmissionWrite,
    ) -> Result<FirstSubmissionAdmission, PortError> {
        let mut state = self.state.lock().expect("lock");
        state.commits += 1;
        if let Some(kind) = state.fail_commit.take() {
            return Err(PortError::Unavailable(kind));
        }
        if let Some(existing) = state.records.get(write.session.as_str()).cloned() {
            return Ok(FirstSubmissionAdmission::AlreadyCommitted(existing));
        }
        let config = ViewJson::empty_object();
        let record = FirstSubmissionRecord {
            session: write.session.clone(),
            first_submission_id: write.first_submission_id.clone(),
            user_message_id: write.user_message_id.clone(),
            first_request_id: write.request_id.clone(),
            current_request_id: write.request_id.clone(),
            stage: acp_core::model::FirstSubmissionStage::SavedNotDispatched,
            ever_dispatched: false,
            agent_session: None,
            resource_generation: None,
            mode_id: write.mode.as_ref().map(|mode| mode.mode_id().clone()),
            config,
            input_digest: write.input.digest().clone(),
            terminal: None,
        };
        state.commands.push(TestFirst::record_command(&write));
        state.messages.insert(
            write.session.as_str().to_owned(),
            FirstUserMessage {
                session: write.session.clone(),
                message: write.user_message_id.clone(),
                turn: Some(write.turn.clone()),
                content: write.input.json().clone(),
                created_at: write.at.clone(),
            },
        );
        state
            .records
            .insert(write.session.as_str().to_owned(), record.clone());
        Ok(FirstSubmissionAdmission::Accepted(record))
    }

    async fn first_submission(
        &self,
        session: SessionId,
    ) -> Result<Option<FirstSubmissionRecord>, PortError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .records
            .get(session.as_str())
            .cloned())
    }

    async fn first_user_message(
        &self,
        session: SessionId,
    ) -> Result<Option<FirstUserMessage>, PortError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .messages
            .get(session.as_str())
            .cloned())
    }

    async fn begin_dispatch(&self, fence: DispatchFence) -> Result<DispatchAdmission, PortError> {
        let mut state = self.state.lock().expect("lock");
        let Some(record) = state.records.get_mut(fence.session.as_str()) else {
            return Ok(DispatchAdmission::NotEligible);
        };
        // 阶段 CAS：只有「可证明未派发」能跨越一次，之后的继续一律 NotEligible。
        if record.ever_dispatched
            || record.stage != acp_core::model::FirstSubmissionStage::SavedNotDispatched
        {
            return Ok(DispatchAdmission::NotEligible);
        }
        record.stage = acp_core::model::FirstSubmissionStage::DispatchStarted;
        record.ever_dispatched = true;
        record.current_request_id = fence.request_id.clone();
        record.agent_session = Some(fence.agent_session.clone());
        record.resource_generation = Some(fence.resource_generation);
        // 与真实存储同一事务：围栏写入该次尝试的命令受理行（重放因此只读原终态）。
        state.commands.push(
            acp_core::model::CommandRecord::try_new(
                Some(fence.session.clone()),
                fence.request_id.clone(),
                "session.start",
                acp_core::model::CommandKind::Mutation,
                fence.actor.clone(),
                Some(fence.at.clone()),
                acp_core::model::CommandStatus::Accepted,
                None,
                None,
                None,
                None,
                None,
                fence.request_fingerprint.clone(),
            )
            .expect("dispatch command row"),
        );
        state.dispatches += 1;
        Ok(DispatchAdmission::Granted)
    }

    async fn settle_first_submission(
        &self,
        settlement: FirstSubmissionSettlement,
    ) -> Result<FirstSubmissionRecord, PortError> {
        let mut state = self.state.lock().expect("lock");
        state.settlements += 1;
        let Some(record) = state.records.get_mut(settlement.session.as_str()) else {
            return Err(PortError::Unavailable(UnavailableKind::IoError));
        };
        record.current_request_id = settlement.request_id.clone();
        record.terminal = Some(acp_core::model::RequestTerminal {
            request_id: settlement.request_id.clone(),
            outcome: settlement.outcome,
            at: settlement.at.clone(),
            error: settlement.error.clone(),
        });
        if settlement.outcome == acp_core::model::RequestOutcome::Failed && !record.ever_dispatched
        {
            // 「可证明未派发的失败」：阶段保持在 `saved_not_dispatched`，正文与首条身份不变。
            record.stage = acp_core::model::FirstSubmissionStage::SavedNotDispatched;
        } else {
            record.stage = acp_core::model::FirstSubmissionStage::Terminal;
        }
        let settled = record.clone();
        state.commands.push(
            acp_core::model::CommandRecord::try_new(
                Some(settlement.session.clone()),
                settlement.request_id,
                "session.start",
                acp_core::model::CommandKind::Mutation,
                settlement.actor,
                Some(settlement.at.clone()),
                acp_core::model::CommandStatus::Failed,
                Some(settlement.at.clone()),
                Some(acp_core::model::EventId::new(&fresh()).expect("terminal event")),
                None,
                settlement.error,
                None,
                settlement.request_fingerprint,
            )
            .expect("failed command row"),
        );
        Ok(settled)
    }

    async fn converge_first_submission(
        &self,
        _session: SessionId,
        _at: Timestamp,
    ) -> Result<FirstSubmissionConvergence, PortError> {
        Ok(FirstSubmissionConvergence::NotPreparation)
    }
}

#[derive(Clone)]
struct FakeTransfer {
    binding: PreparationBinding,
    subject: AttachmentSubject,
    connection: ConnectionId,
    total: u64,
    received: u64,
    media_type: String,
    file_name: String,
    sealed: Option<SealedFacts>,
}

#[derive(Clone)]
struct FakeManifest {
    binding: PreparationBinding,
    subject: AttachmentSubject,
    connection: ConnectionId,
    count: u32,
    transfers: Vec<AttachmentId>,
    sealed_digest: Option<Digest>,
}

/// 与 `storage-sqlite` 同语义的内存暂存替身（只服务本模块的协议用例）。
#[derive(Default)]
pub(crate) struct TestStaging {
    transfers: Mutex<BTreeMap<AttachmentId, FakeTransfer>>,
    manifests: Mutex<BTreeMap<SubmissionId, FakeManifest>>,
}

impl TestStaging {
    fn view(&self, transfer: &AttachmentId, record: &FakeTransfer) -> StagedTransferState {
        StagedTransferState {
            transfer: transfer.clone(),
            binding: record.binding.clone(),
            received: record.received,
            total_length: record.total,
            sealed: record.sealed.clone(),
            bound_connection: record.connection.clone(),
        }
    }

    pub(crate) fn transfer_count(&self) -> usize {
        self.transfers.lock().expect("lock").len()
    }

    fn manifest_view(
        &self,
        submission: &SubmissionId,
        record: &FakeManifest,
    ) -> StagedManifestState {
        StagedManifestState {
            submission: submission.clone(),
            count: record.count,
            registered: record.transfers.clone(),
            sealed_digest: record.sealed_digest.clone(),
        }
    }
}

#[async_trait]
impl PreparationAttachmentStaging for TestStaging {
    async fn begin(&self, begin: StagedTransferBegin) -> Result<StagedTransferState, PortError> {
        let mut transfers = self.transfers.lock().expect("lock");
        if let Some(existing) = transfers.get_mut(&begin.transfer) {
            let same = existing.binding == begin.binding
                && existing.subject == begin.subject
                && existing.total == begin.total_length
                && existing.media_type == begin.immutable.media_type
                && existing.file_name == begin.immutable.file_name;
            if !same {
                return Err(PortError::Conflict(
                    acp_core::model::ConflictKind::AlreadyExists,
                ));
            }
            existing.connection = begin.connection;
            return Ok(self.view(&begin.transfer, existing));
        }
        let record = FakeTransfer {
            binding: begin.binding,
            subject: begin.subject,
            connection: begin.connection,
            total: begin.total_length,
            received: 0,
            media_type: begin.immutable.media_type,
            file_name: begin.immutable.file_name,
            sealed: None,
        };
        let view = self.view(&begin.transfer, &record);
        transfers.insert(begin.transfer, record);
        Ok(view)
    }

    async fn append(&self, chunk: StagedChunk) -> Result<StagedChunkOutcome, PortError> {
        let mut transfers = self.transfers.lock().expect("lock");
        let record = transfers.get_mut(&chunk.transfer).ok_or_else(|| {
            PortError::NotFound(acp_core::model::EntityRef::Attachment(
                chunk.transfer.clone(),
            ))
        })?;
        if record.connection != chunk.connection {
            return Ok(StagedChunkOutcome::ConnectionMismatch);
        }
        if record.sealed.is_some() {
            return Err(PortError::InvalidRequest("transfer is not receiving"));
        }
        if chunk.offset == record.received {
            record.received += chunk.bytes.len() as u64;
            if record.received > record.total {
                return Err(PortError::InvalidRequest(
                    "chunk would exceed the declared total length",
                ));
            }
            return Ok(StagedChunkOutcome::Accepted {
                next_offset: record.received,
            });
        }
        if chunk.offset > record.received {
            return Ok(StagedChunkOutcome::OffsetMismatch {
                expected: record.received,
                actual: chunk.offset,
            });
        }
        Ok(StagedChunkOutcome::Duplicate {
            next_offset: record.received,
        })
    }

    async fn seal(&self, seal: StagedSeal) -> Result<StagedSealOutcome, PortError> {
        let mut transfers = self.transfers.lock().expect("lock");
        let record = transfers.get_mut(&seal.transfer).ok_or_else(|| {
            PortError::NotFound(acp_core::model::EntityRef::Attachment(
                seal.transfer.clone(),
            ))
        })?;
        if record.connection != seal.connection {
            return Ok(StagedSealOutcome::ConnectionMismatch);
        }
        if let Some(facts) = &record.sealed {
            if facts.sha256 == seal.sha256 && facts.length == seal.length {
                return Ok(StagedSealOutcome::AlreadySealed(facts.clone()));
            }
            return Err(PortError::Conflict(
                acp_core::model::ConflictKind::AlreadyExists,
            ));
        }
        if record.received != seal.length || record.total != seal.length {
            return Err(PortError::InvalidRequest(
                "sealed length does not match the received bytes",
            ));
        }
        let facts = SealedFacts {
            transfer: seal.transfer.clone(),
            attachment: seal.transfer.clone(),
            length: seal.length,
            sha256: seal.sha256,
            sealed_at: seal.at,
        };
        record.sealed = Some(facts.clone());
        Ok(StagedSealOutcome::Sealed(facts))
    }

    async fn status(
        &self,
        transfer: AttachmentId,
    ) -> Result<Option<StagedTransferState>, PortError> {
        let transfers = self.transfers.lock().expect("lock");
        Ok(transfers
            .get(&transfer)
            .map(|record| self.view(&transfer, record)))
    }

    async fn abort(
        &self,
        transfer: AttachmentId,
        _reason: StagedAbortReason,
    ) -> Result<(), PortError> {
        self.transfers.lock().expect("lock").remove(&transfer);
        Ok(())
    }

    async fn begin_manifest(
        &self,
        begin: StagedManifestBegin,
    ) -> Result<StagedManifestState, PortError> {
        let mut manifests = self.manifests.lock().expect("lock");
        if let Some(existing) = manifests.get_mut(&begin.submission) {
            let same = existing.binding == begin.binding
                && existing.subject == begin.subject
                && existing.count == begin.count;
            if !same {
                return Err(PortError::Conflict(
                    acp_core::model::ConflictKind::AlreadyExists,
                ));
            }
            existing.connection = begin.connection;
            return Ok(self.manifest_view(&begin.submission, existing));
        }
        let record = FakeManifest {
            binding: begin.binding,
            subject: begin.subject,
            connection: begin.connection,
            count: begin.count,
            transfers: Vec::new(),
            sealed_digest: None,
        };
        let view = self.manifest_view(&begin.submission, &record);
        manifests.insert(begin.submission, record);
        Ok(view)
    }

    async fn append_manifest_page(
        &self,
        page: StagedManifestPage,
    ) -> Result<StagedManifestState, PortError> {
        let mut manifests = self.manifests.lock().expect("lock");
        let manifest = manifests.get_mut(&page.submission).ok_or_else(|| {
            PortError::NotFound(acp_core::model::EntityRef::Submission(
                page.submission.clone(),
            ))
        })?;
        if manifest.connection != page.connection {
            return Err(PortError::Conflict(
                acp_core::model::ConflictKind::AlreadyExists,
            ));
        }
        let transfers = self.transfers.lock().expect("lock");
        for transfer in &page.transfers {
            let record = transfers.get(transfer).ok_or_else(|| {
                PortError::NotFound(acp_core::model::EntityRef::Attachment(transfer.clone()))
            })?;
            if record.binding != manifest.binding
                || record.subject != manifest.subject
                || record.sealed.is_none()
            {
                return Err(PortError::InvalidRequest(
                    "manifest page references an unsealed or foreign transfer",
                ));
            }
        }
        manifest.transfers.extend(page.transfers);
        Ok(self.manifest_view(&page.submission, manifest))
    }

    async fn seal_manifest(
        &self,
        seal: StagedManifestSeal,
    ) -> Result<StagedManifestState, PortError> {
        let mut manifests = self.manifests.lock().expect("lock");
        let manifest = manifests.get_mut(&seal.submission).ok_or_else(|| {
            PortError::NotFound(acp_core::model::EntityRef::Submission(
                seal.submission.clone(),
            ))
        })?;
        if manifest.connection != seal.connection {
            return Err(PortError::Conflict(
                acp_core::model::ConflictKind::AlreadyExists,
            ));
        }
        if manifest.sealed_digest.is_some() {
            return Ok(self.manifest_view(&seal.submission, manifest));
        }
        if manifest.transfers.len() != manifest.count as usize {
            return Err(PortError::InvalidRequest(
                "manifest does not contain the declared number of transfers",
            ));
        }
        manifest.sealed_digest = Some(crate::attachment_transfer::manifest_digest_of(
            manifest.transfers.iter().map(AttachmentId::as_str),
        ));
        Ok(self.manifest_view(&seal.submission, manifest))
    }

    async fn claim(&self, _claim: StagedClaim) -> Result<Vec<ClaimedAttachment>, PortError> {
        Err(PortError::Unavailable(UnavailableKind::BackendUnsupported))
    }

    async fn release_orphans(&self, _before: Timestamp) -> Result<usize, PortError> {
        Ok(0)
    }
}

pub(crate) struct TestHost {
    establish_calls: Mutex<u32>,
    configure_calls: Mutex<u32>,
    claim_calls: Mutex<u32>,
    release_calls: Mutex<u32>,
    releases: Mutex<Vec<(String, u64)>>,
    release_outcome: Mutex<EndpointRelease>,
    options: Mutex<PreparationOptions>,
    /// `claim` 返回的端点（`None` = 该运行期没有可转正的端点，与真实实现「未建立即失败」同形）。
    endpoint: Mutex<Option<Arc<dyn acp_core::ports::SessionEndpoint>>>,
}

impl TestHost {
    fn new() -> Self {
        Self {
            establish_calls: Mutex::new(0),
            configure_calls: Mutex::new(0),
            claim_calls: Mutex::new(0),
            release_calls: Mutex::new(0),
            releases: Mutex::new(Vec::new()),
            release_outcome: Mutex::new(EndpointRelease::Released),
            options: Mutex::new(PreparationOptions {
                modes: ModeState::new(
                    None,
                    vec![
                        ModeRef::try_new(
                            acp_core::model::ModeId::new("default").expect("mode"),
                            "Default",
                        )
                        .expect("mode ref"),
                    ],
                ),
                config: vec![
                    ConfigOption::try_new(
                        acp_core::model::ConfigOptionId::new("model").expect("id"),
                        "Model",
                        None,
                        None,
                        acp_core::model::ConfigOptionKind::Select,
                        acp_core::model::ConfigValue::select("gpt-5").expect("value"),
                        vec![
                            acp_core::model::ConfigOptionEntry::try_new(
                                acp_core::model::SelectValue::new("gpt-5").expect("select"),
                                "GPT-5",
                                None,
                            )
                            .expect("entry"),
                        ],
                    )
                    .expect("config"),
                ],
                blocks_dispatch: false,
            }),
            endpoint: Mutex::new(None),
        }
    }

    pub(crate) fn establish_calls(&self) -> u32 {
        *self.establish_calls.lock().expect("lock")
    }

    pub(crate) fn configure_calls(&self) -> u32 {
        *self.configure_calls.lock().expect("lock")
    }

    pub(crate) fn claim_calls(&self) -> u32 {
        *self.claim_calls.lock().expect("lock")
    }

    pub(crate) fn release_calls(&self) -> u32 {
        *self.release_calls.lock().expect("lock")
    }

    /// 已按资源代释放的端点（顺序即调用顺序）。
    pub(crate) fn releases(&self) -> Vec<(String, u64)> {
        self.releases.lock().expect("lock").clone()
    }

    /// 让 `claim` 返回这份端点（转正成功），并把它交给用例断言派发次数。
    pub(crate) fn set_endpoint(&self, endpoint: Arc<dyn acp_core::ports::SessionEndpoint>) {
        *self.endpoint.lock().expect("lock") = Some(endpoint);
    }

    /// 换掉 `configure`/`establish` 返回的 Agent 确认值（模拟「草稿选择不可用」）。
    pub(crate) fn set_options(&self, options: PreparationOptions) {
        *self.options.lock().expect("lock") = options;
    }
}

#[async_trait]
impl PreparationEndpoints for TestHost {
    async fn establish(
        &self,
        _request: PreparationEstablish,
    ) -> Result<PreparationOptions, PortError> {
        *self.establish_calls.lock().expect("lock") += 1;
        Ok(self.options.lock().expect("lock").clone())
    }

    async fn configure(
        &self,
        _request: PreparationConfigure,
    ) -> Result<PreparationOptions, PortError> {
        *self.configure_calls.lock().expect("lock") += 1;
        Ok(self.options.lock().expect("lock").clone())
    }

    async fn release(&self, session: SessionId, generation: u64) -> EndpointRelease {
        *self.release_calls.lock().expect("lock") += 1;
        self.releases
            .lock()
            .expect("lock")
            .push((session.as_str().to_owned(), generation));
        *self.release_outcome.lock().expect("lock")
    }

    async fn claim(
        &self,
        _session: SessionId,
        _generation: u64,
    ) -> Result<Arc<dyn acp_core::ports::SessionEndpoint>, PortError> {
        *self.claim_calls.lock().expect("lock") += 1;
        match self.endpoint.lock().expect("lock").clone() {
            Some(endpoint) => Ok(endpoint),
            // 未建立端点时显式失败（控制面用例只证明这一条）。
            None => Err(PortError::Unavailable(
                UnavailableKind::SessionNotEstablished,
            )),
        }
    }

    async fn release_all(&self) {}
}

pub(crate) struct Fixture {
    pub(crate) trust: Arc<TestTrust>,
    pub(crate) directory: Arc<PreparationDirectory>,
    pub(crate) first: Arc<TestFirst>,
    pub(crate) staging: Arc<TestStaging>,
    pub(crate) host: Arc<TestHost>,
    pub(crate) preparations: Arc<Preparations>,
    pub(crate) full: Arc<TestCapacity>,
}

pub(crate) fn fixture() -> Fixture {
    let workspace = std::env::temp_dir().to_string_lossy().into_owned();
    let trust = Arc::new(TestTrust::new(&workspace));
    let capacity = Arc::new(TestCapacity {
        full: Mutex::new(false),
        reserves: Mutex::new(Vec::new()),
        releases: Mutex::new(Vec::new()),
    });
    let directory = Arc::new(PreparationDirectory::new(
        Arc::new(TestIds),
        capacity.clone(),
        RuntimeEpoch::new(RUNTIME).expect("runtime epoch"),
    ));
    let first = Arc::new(TestFirst::default());
    let staging = Arc::new(TestStaging::default());
    let host = Arc::new(TestHost::new());
    let preparations = Arc::new(Preparations::new(
        trust.clone() as Arc<dyn PreparationTrust>,
        directory.clone(),
        first.clone() as Arc<dyn FirstSubmission>,
        Some(staging.clone() as Arc<dyn PreparationAttachmentStaging>),
    ));
    Fixture {
        trust,
        directory,
        first,
        staging,
        host,
        preparations,
        full: capacity,
    }
}

pub(crate) fn handle(
    device: &str,
) -> (Arc<ConnectionHandle>, tokio::sync::mpsc::Receiver<Outbound>) {
    ConnectionHandle::new(
        uuid(&fresh()),
        NodeId::new(NODE).expect("node"),
        DeviceId::new(device).expect("device"),
        std::net::IpAddr::from([127, 0, 0, 1]),
        Some(canonical_origin()),
        SessionLimits::negotiate(&SyncConfig::default()),
        Arc::new(TestIds),
    )
}

/// 为已建立的准备签一份 wire receipt（claims 与运行期条目逐项一致）。
pub(crate) fn sign_receipt(
    fixture: &Fixture,
    session: &SessionId,
    tab: &str,
    agent: &str,
) -> sync_protocol::common::PreparationReceipt {
    sign_receipt_with(fixture, session, tab, agent, fixture.trust.workspace())
}

pub(crate) fn sign_receipt_with(
    fixture: &Fixture,
    session: &SessionId,
    tab: &str,
    agent: &str,
    workspace: &str,
) -> sync_protocol::common::PreparationReceipt {
    let claims = PreparationClaims {
        device_id: DeviceId::new(DEVICE).expect("device"),
        owner_node_id: NodeId::new(NODE).expect("node"),
        store_epoch: ServerEpoch::new(EPOCH).expect("epoch"),
        origin: canonical_origin(),
        tab_nonce: nonce(tab),
        session_id: session.clone(),
        agent_id: AgentId::new(agent).expect("agent"),
        normalized_workspace_path: NormalizedWorkspacePath::parse(workspace).expect("path"),
        workspace_source: ReceiptWorkspaceSource::WorkspacePath,
    };
    let receipt = fixture.trust.sign_sync(&claims);
    super::wire_receipt(&receipt).expect("wire receipt")
}

pub(crate) async fn ready_preparation(fixture: &Fixture) -> (SessionId, PreparationView) {
    let owner = PreparationOwner::new(DeviceId::new(DEVICE).expect("device"), nonce(TAB));
    let allocated = fixture
        .directory
        .allocate(
            AllocatePreparation {
                agent: AgentRef::try_new(AgentId::new(AGENT).expect("agent"), AGENT)
                    .expect("agent ref"),
                directory: Some(
                    ResolvedWorkspace::at_path(fixture.trust.workspace().to_owned())
                        .expect("workspace")
                        .directory_id()
                        .clone(),
                ),
                owner,
            },
            now(),
        )
        .await
        .expect("allocate");
    let view = match allocated {
        AllocatePreparationOutcome::Allocated(view) => view,
        AllocatePreparationOutcome::CapacityExceeded => panic!("capacity"),
    };
    let ready = fixture
        .directory
        .mark_ready(&view.session, view.generation, now());
    match ready {
        PreparationDecision::Ready(view) => (view.session.clone(), view),
        other => panic!("not ready: {other:?}"),
    }
}

fn envelope(request: &SessionPreparation) -> Envelope {
    let body = encode_body(request).expect("body");
    Envelope::new(
        MessageType::SessionPreparation,
        uuid(&fresh()),
        Some(ConnectionFields {
            connection_id: uuid(&fresh()),
            connection_sequence: DecimalString::parse("1").expect("sequence"),
        }),
        body,
    )
    .expect("envelope")
}

fn raw_envelope(body: serde_json::Value) -> Envelope {
    let text = serde_json::to_string(&body).expect("json");
    let body = serde_json::value::RawValue::from_string(text).expect("raw");
    Envelope::new(
        MessageType::SessionPreparation,
        uuid(&fresh()),
        Some(ConnectionFields {
            connection_id: uuid(&fresh()),
            connection_sequence: DecimalString::parse("1").expect("sequence"),
        }),
        body,
    )
    .expect("envelope")
}

fn allocate_request(operation: &Uuid) -> SessionPreparation {
    SessionPreparation::new(
        operation.clone(),
        sync_protocol::sync::PreparationAction::Allocate,
        None,
        PreparationRequestDetail::Allocate {
            agent_id: NonEmptyText::<128>::parse(AGENT).expect("agent"),
            workspace: sync_protocol::sync::PreparationWorkspaceRef::Path {
                workspace_path: NonEmptyText::<4096>::parse(
                    std::env::temp_dir().to_string_lossy().as_ref(),
                )
                .expect("path"),
            },
            tab_nonce: Base64Url::<32>::parse(TAB).expect("tab"),
        },
    )
}

fn restore_request(
    operation: &Uuid,
    session: &SessionId,
    receipt: sync_protocol::common::PreparationReceipt,
) -> SessionPreparation {
    SessionPreparation::new(
        operation.clone(),
        sync_protocol::sync::PreparationAction::Restore,
        Some(uuid(session.as_str())),
        PreparationRequestDetail::Restore {
            receipt,
            known_runtime_epoch: Some(uuid(RUNTIME)),
            known_generation: Some(PositiveDecimalString::parse("1").expect("generation")),
        },
    )
}

fn configure_request(
    operation: &Uuid,
    session: &SessionId,
    receipt: sync_protocol::common::PreparationReceipt,
    generation: &str,
    mode: Option<&str>,
) -> SessionPreparation {
    SessionPreparation::new(
        operation.clone(),
        sync_protocol::sync::PreparationAction::Configure,
        Some(uuid(session.as_str())),
        PreparationRequestDetail::Configure {
            receipt,
            runtime_epoch: uuid(RUNTIME),
            generation: PositiveDecimalString::parse(generation).expect("generation"),
            mode: mode.map(|mode| NonEmptyText::<256>::parse(mode).expect("mode")),
            config_options: None,
        },
    )
}

// ---------------------------------------------------------------------------------------------
// 用例
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn unnegotiated_preparation_frame_is_type_unsupported() {
    let fixture = fixture();
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let (handle, mut receiver) = handle(DEVICE);
    let request = allocate_request(&uuid(&fresh()));
    let outcome = route.route(&handle, &envelope(&request)).await;
    assert_eq!(outcome, RouteOutcome::Claimed);
    let sent = receiver.try_recv().expect("error frame");
    let envelope = Envelope::decode(&sent.text).expect("envelope");
    let body: sync_protocol::error::Body =
        serde_json::from_str(envelope.body().get()).expect("error body");
    assert_eq!(body.code, ErrorCode::ProtocolTypeUnsupported);
    assert_eq!(fixture.host.establish_calls(), 0);
    assert_eq!(fixture.staging.transfer_count(), 0);
}

#[tokio::test]
async fn device_revoked_is_refused_before_any_preparation_action() {
    let fixture = fixture();
    fixture.trust.set_credential(CredentialStatus::Revoked);
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    let request = allocate_request(&uuid(&fresh()));
    route.route(&handle, &envelope(&request)).await;
    let sent = receiver.try_recv().expect("error frame");
    let envelope = Envelope::decode(&sent.text).expect("envelope");
    let body: sync_protocol::error::Body =
        serde_json::from_str(envelope.body().get()).expect("error body");
    assert_eq!(body.code, ErrorCode::AuthDeviceRevoked);
    assert_eq!(
        handle.close_request().map(|(code, _)| code),
        Some(close::REVOKED)
    );
    assert_eq!(fixture.host.establish_calls(), 0);
}

#[tokio::test]
async fn allocate_uses_owner_allocated_id_and_returns_ready() {
    let fixture = fixture();
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    let request = allocate_request(&uuid(&fresh()));
    route.route(&handle, &envelope(&request)).await;
    let sent = receiver.try_recv().expect("result frame");
    let envelope = Envelope::decode(&sent.text).expect("envelope");
    let result: SessionPreparationResult =
        serde_json::from_str(envelope.body().get()).expect("result");
    assert_eq!(result.operation_id, request.operation_id);
    assert_eq!(
        result.action,
        sync_protocol::sync::PreparationAction::Allocate
    );
    match result.outcome {
        PreparationOutcome::Ready {
            runtime_epoch,
            generation,
            modes,
            ..
        } => {
            assert_eq!(runtime_epoch.as_str(), RUNTIME);
            assert_eq!(generation.as_str(), "1");
            assert_eq!(modes.len(), 1);
        }
        other => panic!("expected ready, got {other:?}"),
    }
    assert!(
        fixture
            .directory
            .view(&SessionId::new(result.session_id.as_str()).expect("session"))
            .is_some()
    );
    assert_eq!(fixture.host.establish_calls(), 1);
}

#[tokio::test]
async fn allocate_with_client_session_id_is_schema_invalid() {
    let fixture = fixture();
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    let body = serde_json::json!({
        "operationId": fresh(),
        "action": "allocate",
        "sessionId": fresh(),
        "agentId": AGENT,
        "workspace": { "workspacePath": std::env::temp_dir().to_string_lossy() },
        "tabNonce": TAB,
    });
    route.route(&handle, &raw_envelope(body)).await;
    let sent = receiver.try_recv().expect("error frame");
    let envelope = Envelope::decode(&sent.text).expect("envelope");
    let body: sync_protocol::error::Body =
        serde_json::from_str(envelope.body().get()).expect("error body");
    assert_eq!(body.code, ErrorCode::ProtocolSchemaInvalid);
    assert_eq!(fixture.host.establish_calls(), 0);
}

#[tokio::test]
async fn same_operation_same_parameters_is_idempotent() {
    let fixture = fixture();
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    let request = allocate_request(&uuid(&fresh()));
    let envelope = envelope(&request);
    route.route(&handle, &envelope).await;
    route.route(&handle, &envelope).await;
    let first: SessionPreparationResult = serde_json::from_str(
        Envelope::decode(&receiver.try_recv().expect("first").text)
            .expect("envelope")
            .body()
            .get(),
    )
    .expect("result");
    let second: SessionPreparationResult = serde_json::from_str(
        Envelope::decode(&receiver.try_recv().expect("second").text)
            .expect("envelope")
            .body()
            .get(),
    )
    .expect("result");
    assert_eq!(first, second);
    assert_eq!(
        fixture.host.establish_calls(),
        1,
        "the second frame must not establish again"
    );
}

#[tokio::test]
async fn same_operation_different_parameters_is_rejected() {
    let fixture = fixture();
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    let operation = uuid(&fresh());
    route
        .route(&handle, &envelope(&allocate_request(&operation)))
        .await;
    let mut reconfigured = allocate_request(&operation);
    reconfigured.detail = PreparationRequestDetail::Allocate {
        agent_id: NonEmptyText::<128>::parse("other").expect("agent"),
        workspace: sync_protocol::sync::PreparationWorkspaceRef::Path {
            workspace_path: NonEmptyText::<4096>::parse(
                std::env::temp_dir().to_string_lossy().as_ref(),
            )
            .expect("path"),
        },
        tab_nonce: Base64Url::<32>::parse(TAB).expect("tab"),
    };
    route.route(&handle, &envelope(&reconfigured)).await;
    let _first = receiver.try_recv().expect("first result");
    let sent = receiver.try_recv().expect("conflict");
    let envelope = Envelope::decode(&sent.text).expect("envelope");
    let body: sync_protocol::error::Body =
        serde_json::from_str(envelope.body().get()).expect("error body");
    assert_eq!(body.code, ErrorCode::CommandIdempotencyConflict);
    assert_eq!(fixture.host.establish_calls(), 1);
}

#[tokio::test]
async fn restore_rejects_tampered_and_cross_device_receipts() {
    let fixture = fixture();
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let (session, _view) = ready_preparation(&fixture).await;
    let mut receipt = sign_receipt(&fixture, &session, TAB, AGENT);
    receipt.claims.session_id = uuid(&fresh());

    let (connection, mut receiver) = handle(DEVICE);
    route.on_authenticated(&connection, &[PREPARATION_FEATURE.to_owned()]);
    route
        .route(
            &connection,
            &envelope(&restore_request(&uuid(&fresh()), &session, receipt)),
        )
        .await;
    let sent = receiver.try_recv().expect("binding error");
    let received = Envelope::decode(&sent.text).expect("envelope");
    let body: sync_protocol::error::Body =
        serde_json::from_str(received.body().get()).expect("error body");
    assert_eq!(body.code, ErrorCode::AttachmentBindingMismatch);

    // 另一台设备的连接使用同一份 receipt：绑定不符（不泄露是哪一项失败）。
    let receipt = sign_receipt(&fixture, &session, TAB, AGENT);
    let (other, mut other_receiver) = handle(OTHER_DEVICE);
    route.on_authenticated(&other, &[PREPARATION_FEATURE.to_owned()]);
    route
        .route(
            &other,
            &envelope(&restore_request(&uuid(&fresh()), &session, receipt)),
        )
        .await;
    let sent = other_receiver.try_recv().expect("binding error");
    let received = Envelope::decode(&sent.text).expect("envelope");
    let body: sync_protocol::error::Body =
        serde_json::from_str(received.body().get()).expect("error body");
    assert_eq!(body.code, ErrorCode::AttachmentBindingMismatch);
}

#[tokio::test]
async fn restore_after_store_rebuild_is_not_retryable() {
    let fixture = fixture();
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let (session, _view) = ready_preparation(&fixture).await;
    let receipt = sign_receipt_with(&fixture, &session, TAB, AGENT, fixture.trust.workspace());
    // 篡改 storeEpoch：签名覆盖它，因此必须重签一份「另一存储代」的合法 claim 才能走到 store_rebuilt。
    let mut claims = PreparationClaims {
        device_id: DeviceId::new(DEVICE).expect("device"),
        owner_node_id: NodeId::new(NODE).expect("node"),
        store_epoch: ServerEpoch::new(REBUILT_EPOCH).expect("epoch"),
        origin: canonical_origin(),
        tab_nonce: nonce(TAB),
        session_id: session.clone(),
        agent_id: AgentId::new(AGENT).expect("agent"),
        normalized_workspace_path: NormalizedWorkspacePath::parse(fixture.trust.workspace())
            .expect("path"),
        workspace_source: ReceiptWorkspaceSource::WorkspacePath,
    };
    claims.store_epoch = ServerEpoch::new(REBUILT_EPOCH).expect("epoch");
    let signed = fixture.trust.sign_sync(&claims);
    let rebuilt = super::wire_receipt(&signed).expect("wire");
    let _ = receipt;

    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    route
        .route(
            &handle,
            &envelope(&restore_request(&uuid(&fresh()), &session, rebuilt)),
        )
        .await;
    let sent = receiver.try_recv().expect("store rebuilt");
    let envelope = Envelope::decode(&sent.text).expect("envelope");
    // storeEpoch 失效是准备结果的 `failed` 分支（D3.1），不是认证/协议 error 帧。
    let result: SessionPreparationResult =
        serde_json::from_str(envelope.body().get()).expect("result");
    match result.outcome {
        PreparationOutcome::Failed { error } => {
            assert_eq!(error.code, ErrorCode::PreparationStoreRebuilt);
            assert!(!error.retryable);
        }
        other => panic!("expected failed store_rebuilt, got {other:?}"),
    }
}

#[tokio::test]
async fn committed_takes_precedence_over_stale_generation() {
    let fixture = fixture();
    let (session, _view) = ready_preparation(&fixture).await;
    fixture.first.set(committed_record(&session));
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let receipt = sign_receipt(&fixture, &session, TAB, AGENT);
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    route
        .route(
            &handle,
            &envelope(&configure_request(
                &uuid(&fresh()),
                &session,
                receipt,
                "99",
                Some("plan"),
            )),
        )
        .await;
    let sent = receiver.try_recv().expect("result");
    let envelope = Envelope::decode(&sent.text).expect("envelope");
    let result: SessionPreparationResult =
        serde_json::from_str(envelope.body().get()).expect("result");
    assert!(matches!(
        result.outcome,
        PreparationOutcome::Committed { .. }
    ));
}

#[tokio::test]
async fn generation_mismatch_is_stale() {
    let fixture = fixture();
    let (session, _view) = ready_preparation(&fixture).await;
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let receipt = sign_receipt(&fixture, &session, TAB, AGENT);
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    route
        .route(
            &handle,
            &envelope(&configure_request(
                &uuid(&fresh()),
                &session,
                receipt,
                "99",
                Some("plan"),
            )),
        )
        .await;
    let sent = receiver.try_recv().expect("result");
    let envelope = Envelope::decode(&sent.text).expect("envelope");
    let result: SessionPreparationResult =
        serde_json::from_str(envelope.body().get()).expect("result");
    match result.outcome {
        PreparationOutcome::Stale { reason } => {
            assert_eq!(reason, WireStaleReason::GenerationMismatch)
        }
        other => panic!("expected stale, got {other:?}"),
    }
}

#[tokio::test]
async fn release_after_commit_is_committed_and_does_not_touch_the_endpoint() {
    let fixture = fixture();
    let (session, view) = ready_preparation(&fixture).await;
    fixture.first.set(committed_record(&session));
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let receipt = sign_receipt(&fixture, &session, TAB, AGENT);
    let request = SessionPreparation::new(
        uuid(&fresh()),
        sync_protocol::sync::PreparationAction::Release,
        Some(uuid(session.as_str())),
        PreparationRequestDetail::Release {
            receipt,
            runtime_epoch: uuid(RUNTIME),
            generation: PositiveDecimalString::parse(&view.generation.get().to_string())
                .expect("generation"),
        },
    );
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    route.route(&handle, &envelope(&request)).await;
    let sent = receiver.try_recv().expect("result");
    let envelope = Envelope::decode(&sent.text).expect("envelope");
    let result: SessionPreparationResult =
        serde_json::from_str(envelope.body().get()).expect("result");
    assert!(matches!(
        result.outcome,
        PreparationOutcome::Committed { .. }
    ));
    assert_eq!(fixture.host.release_calls(), 0);
    assert!(fixture.directory.view(&session).is_some());
}

#[tokio::test]
async fn release_is_idempotent_and_reports_released() {
    let fixture = fixture();
    let (session, view) = ready_preparation(&fixture).await;
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let receipt = sign_receipt(&fixture, &session, TAB, AGENT);
    let generation = PositiveDecimalString::parse(&view.generation.get().to_string()).expect("gen");
    let request = |operation: Uuid| {
        SessionPreparation::new(
            operation,
            sync_protocol::sync::PreparationAction::Release,
            Some(uuid(session.as_str())),
            PreparationRequestDetail::Release {
                receipt: receipt.clone(),
                runtime_epoch: uuid(RUNTIME),
                generation: generation.clone(),
            },
        )
    };
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    route
        .route(&handle, &envelope(&request(uuid(&fresh()))))
        .await;
    route
        .route(&handle, &envelope(&request(uuid(&fresh()))))
        .await;
    for _ in 0..2 {
        let sent = receiver.try_recv().expect("result");
        let envelope = Envelope::decode(&sent.text).expect("envelope");
        let result: SessionPreparationResult =
            serde_json::from_str(envelope.body().get()).expect("result");
        assert!(matches!(result.outcome, PreparationOutcome::Released));
    }
    assert!(fixture.directory.view(&session).is_none());
}

#[tokio::test]
async fn capacity_exceeded_is_reported_without_allocating() {
    let fixture = fixture();
    *fixture.full.full.lock().expect("lock") = true;
    let route = PreparationRoute::new(fixture.preparations.clone(), fixture.host.clone());
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    route
        .route(&handle, &envelope(&allocate_request(&uuid(&fresh()))))
        .await;
    let sent = receiver.try_recv().expect("error");
    let envelope = Envelope::decode(&sent.text).expect("envelope");
    let body: sync_protocol::error::Body =
        serde_json::from_str(envelope.body().get()).expect("error body");
    assert_eq!(body.code, ErrorCode::SessionCapacityExceeded);
    assert!(body.retryable);
    assert_eq!(fixture.host.establish_calls(), 0);
}

#[tokio::test]
async fn control_rate_is_bounded_per_connection() {
    let fixture = fixture();
    let route = PreparationRoute::for_tests(fixture.preparations.clone(), fixture.host.clone(), 2);
    let (handle, mut receiver) = handle(DEVICE);
    route.on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
    for _ in 0..3 {
        route
            .route(&handle, &envelope(&allocate_request(&uuid(&fresh()))))
            .await;
    }
    let mut codes = Vec::new();
    while let Ok(sent) = receiver.try_recv() {
        let envelope = Envelope::decode(&sent.text).expect("envelope");
        if envelope.message_type() == MessageType::Error {
            let body: sync_protocol::error::Body =
                serde_json::from_str(envelope.body().get()).expect("error body");
            codes.push(body.code);
        }
    }
    assert_eq!(
        codes.last().copied(),
        Some(ErrorCode::ResourceRateLimited),
        "the third frame must be rate limited"
    );
}

// ---------------------------------------------------------------------------------------------
// 构造首条记录（committed 分支）
// ---------------------------------------------------------------------------------------------

pub(crate) fn committed_record(session: &SessionId) -> FirstSubmissionRecord {
    FirstSubmissionRecord {
        session: session.clone(),
        first_submission_id: FirstSubmissionId::new(&fresh()).expect("first submission"),
        user_message_id: acp_core::model::MessageId::new(&fresh()).expect("message"),
        first_request_id: acp_core::model::RequestId::new(&fresh()).expect("request"),
        current_request_id: acp_core::model::RequestId::new(&fresh()).expect("request"),
        stage: acp_core::model::FirstSubmissionStage::SavedNotDispatched,
        ever_dispatched: false,
        agent_session: None,
        resource_generation: None,
        mode_id: None,
        config: ViewJson::empty_object(),
        input_digest: Digest::new(&"A".repeat(43)).expect("digest"),
        terminal: None,
    }
}
