//! `session.start` 的**处理器级**用例（`web-session-preparation` D7/D8/D8.1；任务 5.5）。
//!
//! 与 `preparation_tests` 的分工：那里断言控制帧（allocate/restore/configure/renew/release），这里
//! 断言真正的命令处理器 [`CommandRoute::on_session_start`]——`kind: initial` 的受理/派发/资源转移与
//! `kind: continue` 的准入顺序。两处共用同一份窄替身（`preparation::tests` 的 `TestHost`/`TestFirst`/
//! `TestPreparation`），因此「冻结→提交→转移」的事实只在一条装配里被观察。
//!
//! 装配形状与组合根一致：同一个 `Arc<TestFirst>` 同时接在 `Preparations` 与 `UseCases`/`Broker` 上
//! （首条受理与 committed 投影必须同源），`Broker` 接真实 `LocalSessionLifecycle`；只有存储、后端、
//! 端点与信任是窄替身。断言落在**消费者可见的事实**上：命令回包的状态、持久首条/命令行的份数、派发
//! 计数、准备目录的占位与容量账、生命周期里的 ready 资源。
//!
//! 刻意不做的两件事：
//! - 不重放 core 的 CAS/事务语义（那是 WP2 的用例）；这里用替身驱动分支，断言**处理器**在每条分支上
//!   对准备资源做了什么。
//! - 不断言源码文本或「没有 panic」；每条断言都能被反向实现打红。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::{
    Actor, AgentId, AgentRef, AgentSessionId, AuditRecord, ConfigOption, ConfigOptionId,
    ConfigValue, CreateSessionRequest, DeviceId, DeviceRecord, DeviceState, DirectoryId,
    DirectoryRecord, EventId, EventPayload, FirstSubmissionRecord, FirstSubmissionStage, ModeRef,
    ModeState, OriginEpoch, OwnedSessionRef, PeerPublicKey, PortError, PublicError, RequestId,
    RequestOutcome, ResolvedWorkspace, ResourceGeneration, ResourceOrigin, ResumeSessionRequest,
    SessionRecoveryRecord, SessionReference, SessionSnapshot, SessionState, SessionSummary, Turn,
    TurnId, UnavailableKind, Version, ViewJson, WorkspaceRef,
};
use acp_core::ports::{
    AgentCatalog, AttachmentStore, AuditQuery, AuditStore, Clock, CommitOutcome, EventPublisher,
    FirstSubmission, HistoryPage, HistoryQuery, LocalConfigStore, PruneReport, ReplayBatch,
    ReplayLimit, SessionActivityFacts, SessionBackendFactory, SessionEndpoint, SessionQuery,
    SessionStore, SyncReplaySlice, TurnAccepted,
};
use acp_core::use_cases::{
    ActivationPlan, LifecycleConfig, LocalSessionLifecycle, UseCaseDeps, UseCases,
};
use async_trait::async_trait;
use identity_auth::{
    EntropyError, EntropySource, IdentityKeystore, KeyHandle, KeyPurpose, KeystoreError,
    P1363Signature, SecretBytes, SecretPurpose,
};
use sync_protocol::command::{CommandName, CommandResult, CommandState};
use sync_protocol::common::DecimalString;
use sync_protocol::envelope::{ConnectionFields, Envelope, MessageType};

use super::*;
use crate::sync::conn::registry::{ConnectionRegistry, Outbound};
use crate::sync::preparation::tests as fixture_tests;
use crate::sync::subscribe::OwnerReachability;

/// 固定时间源（与准备面的 `TestTrust::now` 同值：租约判定必须看到同一个时钟）。
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> acp_core::model::Timestamp {
        acp_core::model::Timestamp::new(fixture_tests::AT).expect("timestamp")
    }
}

/// 与测试用的节点身份同一把 P-256 密钥（准备凭据的验签走真实验签路径）。
fn test_public_key() -> PeerPublicKey {
    let signing = p256::ecdsa::SigningKey::from_slice(&[7u8; 32]).expect("fixed scalar");
    let verifying = p256::ecdsa::VerifyingKey::from(&signing);
    let point = verifying.to_sec1_point(false);
    PeerPublicKey::try_from_bytes(point.as_bytes()).expect("public key")
}

/// keystore 替身：只需要交出节点公钥（处理器此时不签名）。
struct TestKeystore {
    public_key: PeerPublicKey,
}

#[async_trait]
impl IdentityKeystore for TestKeystore {
    async fn generate(
        &self,
        _purpose: KeyPurpose,
        _label: &str,
    ) -> Result<KeyHandle, KeystoreError> {
        Err(KeystoreError::Unavailable)
    }

    async fn public_key(&self, _handle: &KeyHandle) -> Result<PeerPublicKey, KeystoreError> {
        Ok(self.public_key.clone())
    }

    async fn sign(
        &self,
        _handle: &KeyHandle,
        _transcript: &[u8],
    ) -> Result<P1363Signature, KeystoreError> {
        Err(KeystoreError::Unavailable)
    }

    async fn delete(&self, _handle: &KeyHandle) -> Result<(), KeystoreError> {
        Err(KeystoreError::Unavailable)
    }

    async fn get_secret(
        &self,
        _purpose: SecretPurpose,
        _key: &str,
    ) -> Result<Option<SecretBytes>, KeystoreError> {
        Err(KeystoreError::Unavailable)
    }

    async fn put_secret(
        &self,
        _purpose: SecretPurpose,
        _key: &str,
        _value: &SecretBytes,
    ) -> Result<(), KeystoreError> {
        Err(KeystoreError::Unavailable)
    }

    async fn delete_secret(
        &self,
        _purpose: SecretPurpose,
        _key: &str,
    ) -> Result<(), KeystoreError> {
        Err(KeystoreError::Unavailable)
    }
}

struct ZeroEntropy;

impl EntropySource for ZeroEntropy {
    fn fill(&self, out: &mut [u8]) -> Result<(), EntropyError> {
        out.fill(0);
        Ok(())
    }
}

fn unavailable() -> PortError {
    PortError::Unavailable(UnavailableKind::BackendUnsupported)
}

fn cursor() -> acp_core::model::GlobalCursor {
    acp_core::model::GlobalCursor::new(
        acp_core::model::ServerEpoch::new(fixture_tests::EPOCH).expect("epoch"),
        acp_core::model::Sequence::new(0).expect("sequence"),
    )
}

fn at() -> acp_core::model::Timestamp {
    acp_core::model::Timestamp::new(fixture_tests::AT).expect("timestamp")
}

fn agent() -> AgentRef {
    AgentRef::try_new(
        AgentId::new(fixture_tests::AGENT).expect("agent"),
        fixture_tests::AGENT,
    )
    .expect("agent ref")
}

fn directory_id(workspace: &str) -> DirectoryId {
    ResolvedWorkspace::at_path(workspace.to_owned())
        .expect("workspace")
        .directory_id()
        .clone()
}

// ---------------------------------------------------------------------------------------------
// core 侧的窄替身
// ---------------------------------------------------------------------------------------------

/// 只服务 `session.start` 这条路径的存储替身：
/// - `find_request`/`load`/`list` 是处理器会读的三项持久事实；
/// - `commit_with_submission` 是派发围栏与 turn 事实的落点（只记次数与版本，不重放事务语义）。
struct TestStore {
    first: Arc<fixture_tests::TestFirst>,
    session: SessionId,
    directory: DirectoryId,
    version: Mutex<u64>,
    commits: AtomicU32,
}

impl TestStore {
    fn new(session: SessionId, workspace: &str, first: Arc<fixture_tests::TestFirst>) -> Self {
        Self {
            first,
            session: session.clone(),
            directory: directory_id(workspace),
            version: Mutex::new(1),
            commits: AtomicU32::new(0),
        }
    }

    fn summary(&self) -> SessionSummary {
        SessionSummary::try_new(
            self.session.clone(),
            None,
            agent(),
            SessionState::Idle,
            ResourceOrigin::Local,
            None,
            Version::new(1),
            at(),
            at(),
            Some(WorkspaceRef::new(self.directory.clone())),
        )
        .expect("summary")
    }

    fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            session: acp_core::model::Session::try_new(
                self.session.clone(),
                OwnedSessionRef::new(self.session.clone()),
                None,
                None,
                agent(),
                SessionState::Idle,
                ResourceOrigin::Local,
                None,
                Version::new(*self.version.lock().expect("lock")),
                at(),
                at(),
            )
            .expect("session"),
            origin_epoch: OriginEpoch::new(&uuid_text()).expect("origin epoch"),
            head: cursor(),
        }
    }
}

#[async_trait]
impl SessionStore for TestStore {
    async fn commit(
        &self,
        commit: acp_core::ports::OwnedCommit,
    ) -> Result<CommitOutcome, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        let mut version = self.version.lock().expect("lock");
        if commit.state.is_some() {
            *version += 1;
        }
        Ok(CommitOutcome {
            session_id: commit.session.clone(),
            origin_epoch: None,
            version: Version::new(*version),
            appended: Vec::new(),
            replayed: None,
        })
    }

    async fn session_activity(
        &self,
        _session: &SessionId,
    ) -> Result<SessionActivityFacts, PortError> {
        Ok(SessionActivityFacts::default())
    }

    async fn queued_turns(&self, _session: &SessionId) -> Result<Vec<Turn>, PortError> {
        Ok(Vec::new())
    }

    async fn sessions_with_pending_activity(
        &self,
        _limit: ReplayLimit,
    ) -> Result<Vec<SessionId>, PortError> {
        Ok(Vec::new())
    }

    async fn load(&self, session: &SessionId) -> Result<Option<SessionSnapshot>, PortError> {
        Ok((session == &self.session).then(|| self.snapshot()))
    }

    async fn list(&self, _query: SessionQuery) -> Result<Vec<SessionSummary>, PortError> {
        Ok(vec![self.summary()])
    }

    async fn origin_epochs(
        &self,
        sessions: &[SessionId],
    ) -> Result<Vec<(SessionId, OriginEpoch)>, PortError> {
        Ok(sessions
            .iter()
            .filter(|session| *session == &self.session)
            .map(|session| {
                (
                    session.clone(),
                    OriginEpoch::new(&uuid_text()).expect("origin epoch"),
                )
            })
            .collect())
    }

    async fn head(&self) -> Result<acp_core::model::GlobalCursor, PortError> {
        Ok(cursor())
    }

    async fn read_view(&self) -> Result<Box<dyn acp_core::ports::ReadView>, PortError> {
        Ok(Box::new(EmptyView))
    }

    async fn find_request(
        &self,
        request: &RequestId,
        _actor: &Actor,
    ) -> Result<Option<acp_core::model::CommandRecord>, PortError> {
        Ok(self
            .first
            .state()
            .lock()
            .expect("lock")
            .commands
            .iter()
            .find(|record| record.request() == request)
            .cloned())
    }

    async fn load_recovery(
        &self,
        _session: &SessionId,
    ) -> Result<Option<SessionRecoveryRecord>, PortError> {
        Ok(None)
    }

    async fn unsettled_commands(
        &self,
        _limit: ReplayLimit,
    ) -> Result<Vec<acp_core::model::CommandRecord>, PortError> {
        Ok(Vec::new())
    }

    async fn retention_window(
        &self,
        _session: &SessionId,
    ) -> Result<Option<(acp_core::model::Sequence, acp_core::model::Sequence)>, PortError> {
        Ok(None)
    }

    async fn prune(
        &self,
        _policy: acp_core::ports::RetentionPolicy,
        _at: acp_core::model::Timestamp,
    ) -> Result<acp_core::ports::PruneReport, PortError> {
        Ok(acp_core::ports::PruneReport::default())
    }

    async fn health(&self) -> Result<acp_core::ports::StoreHealth, PortError> {
        Err(unavailable())
    }
}

/// 只有一条一致性 head 的空读视图（启动收敛不读任何已发布事件）。
struct EmptyView;

#[async_trait]
impl acp_core::ports::ReadView for EmptyView {
    async fn head(&self) -> Result<acp_core::model::GlobalCursor, PortError> {
        Ok(cursor())
    }

    async fn replay(
        &self,
        _after: Option<acp_core::model::GlobalCursor>,
        _limit: ReplayLimit,
    ) -> Result<ReplayBatch, PortError> {
        Err(unavailable())
    }

    async fn sync_replay(
        &self,
        _after: Option<acp_core::model::GlobalCursor>,
        _limit: ReplayLimit,
    ) -> Result<SyncReplaySlice, PortError> {
        Ok(SyncReplaySlice {
            events: Vec::new(),
            head: cursor(),
        })
    }

    async fn read_session(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
        Err(unavailable())
    }

    async fn read_session_origin(
        &self,
        _query: HistoryQuery,
    ) -> Result<acp_core::ports::OriginReadPage, PortError> {
        Err(unavailable())
    }

    async fn event_payload(&self, _event: &EventId) -> Result<Option<EventPayload>, PortError> {
        Ok(None)
    }

    async fn node_link_slice(
        &self,
        _session: &SessionId,
        _after: Option<acp_core::model::OriginCursor>,
        _limit: ReplayLimit,
    ) -> Result<acp_core::ports::NodeLinkSlice, PortError> {
        Err(unavailable())
    }

    async fn session_event_payload(
        &self,
        _session: &SessionId,
        _event: &EventId,
    ) -> Result<Option<EventPayload>, PortError> {
        Ok(None)
    }
}

/// 命令准入的持久事实：只回答「该设备是否已登记」。
struct TestCoreTrust {
    device: DeviceRecord,
}

impl TestCoreTrust {
    fn new() -> Self {
        let public_key = test_public_key();
        Self {
            device: DeviceRecord::try_new(
                DeviceId::new(fixture_tests::DEVICE).expect("device"),
                "test device",
                public_key.fingerprint(),
                DeviceState::Active,
                at(),
                None,
                None,
            )
            .expect("device record"),
        }
    }
}

#[async_trait]
impl acp_core::ports::TrustStore for TestCoreTrust {
    async fn device(&self, id: &DeviceId) -> Result<Option<DeviceRecord>, PortError> {
        Ok((id == self.device.device_id()).then(|| self.device.clone()))
    }

    async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError> {
        Ok(vec![self.device.clone()])
    }

    async fn node(
        &self,
        _id: &acp_core::model::NodeId,
        _kind: acp_core::model::NodeKind,
    ) -> Result<Option<acp_core::model::NodeRecord>, PortError> {
        Ok(None)
    }

    async fn nodes(&self) -> Result<Vec<acp_core::model::NodeRecord>, PortError> {
        Ok(Vec::new())
    }

    async fn nodes_for(
        &self,
        _id: &acp_core::model::NodeId,
    ) -> Result<Vec<acp_core::model::NodeRecord>, PortError> {
        Ok(Vec::new())
    }

    async fn peer_key(
        &self,
        _peer: &acp_core::model::PeerIdentity,
    ) -> Result<Option<PeerPublicKey>, PortError> {
        Ok(None)
    }

    async fn pairing(
        &self,
        _id: &acp_core::model::PairingId,
    ) -> Result<Option<acp_core::model::PairingRecord>, PortError> {
        Ok(None)
    }

    async fn pairings(&self) -> Result<Vec<acp_core::model::PairingRecord>, PortError> {
        Ok(Vec::new())
    }

    async fn pairing_peer(
        &self,
        _id: &acp_core::model::PairingId,
    ) -> Result<Option<acp_core::model::PairingPeer>, PortError> {
        Ok(None)
    }

    async fn pairing_for(
        &self,
        _peer: &acp_core::model::PeerIdentity,
    ) -> Result<Option<acp_core::model::PairingRecord>, PortError> {
        Ok(None)
    }

    async fn put_device(&self, _write: acp_core::ports::DeviceWrite) -> Result<(), PortError> {
        Err(unavailable())
    }

    async fn put_node(&self, _write: acp_core::ports::NodeWrite) -> Result<(), PortError> {
        Err(unavailable())
    }

    async fn revoke_device(
        &self,
        _write: acp_core::ports::DeviceRevocation,
    ) -> Result<(), PortError> {
        Err(unavailable())
    }

    async fn revoke_node(&self, _write: acp_core::ports::NodeRevocation) -> Result<(), PortError> {
        Err(unavailable())
    }

    async fn create_pairing(&self, _write: acp_core::ports::PairingWrite) -> Result<(), PortError> {
        Err(unavailable())
    }

    async fn claim_pairing(
        &self,
        _write: acp_core::ports::PairingClaimWrite,
    ) -> Result<acp_core::ports::PairingClaimOutcome, PortError> {
        Err(unavailable())
    }

    async fn settle_pairing(
        &self,
        _write: acp_core::ports::PairingSettlementWrite,
    ) -> Result<acp_core::ports::TrustRecordRef, PortError> {
        Err(unavailable())
    }

    async fn expire_pairings(
        &self,
        _write: acp_core::ports::ExpiryWrite,
    ) -> Result<u64, PortError> {
        Err(unavailable())
    }

    async fn consume_pairing(
        &self,
        _write: acp_core::ports::PairingConsumption,
    ) -> Result<acp_core::model::PairingRecord, PortError> {
        Err(unavailable())
    }

    async fn record_node_connected(
        &self,
        _write: acp_core::ports::NodeConnectedWrite,
    ) -> Result<(), PortError> {
        Err(unavailable())
    }
}

/// 目录读取：会话的工作目录仍是已登记目录（`session_workspace` 的唯一持久来源）。
struct TestConfig {
    record: DirectoryRecord,
}

#[async_trait]
impl LocalConfigStore for TestConfig {
    async fn directories(&self) -> Result<Vec<DirectoryRecord>, PortError> {
        Ok(vec![self.record.clone()])
    }

    async fn directory(
        &self,
        directory_id: &DirectoryId,
    ) -> Result<Option<DirectoryRecord>, PortError> {
        Ok((directory_id == self.record.directory_id()).then(|| self.record.clone()))
    }
}

struct TestAudit;

#[async_trait]
impl AuditStore for TestAudit {
    async fn append(&self, _record: AuditRecord) -> Result<(), PortError> {
        Ok(())
    }

    async fn query(&self, _query: AuditQuery) -> Result<Vec<AuditRecord>, PortError> {
        Ok(Vec::new())
    }

    async fn watermark(&self) -> Result<u64, PortError> {
        Ok(0)
    }
}

struct TestAttachments;

#[async_trait]
impl AttachmentStore for TestAttachments {
    async fn put(
        &self,
        _bytes: &[u8],
        _media_type: &str,
        _at: acp_core::model::Timestamp,
    ) -> Result<acp_core::ports::AttachmentRef, PortError> {
        Err(unavailable())
    }

    async fn get(&self, _id: &AttachmentId) -> Result<Option<Vec<u8>>, PortError> {
        Ok(None)
    }

    async fn link(
        &self,
        _session: &SessionId,
        _attachment: &AttachmentId,
        _generation: acp_core::model::AttachmentGeneration,
        _at: acp_core::model::Timestamp,
    ) -> Result<(), PortError> {
        Err(unavailable())
    }

    async fn prune_lru(
        &self,
        _budget_bytes: u64,
        _at: acp_core::model::Timestamp,
    ) -> Result<PruneReport, PortError> {
        Ok(PruneReport::default())
    }

    async fn sweep_orphans(
        &self,
        _at: acp_core::model::Timestamp,
        _limit: u32,
    ) -> Result<u32, PortError> {
        Ok(0)
    }
}

struct TestCatalog;

#[async_trait]
impl AgentCatalog for TestCatalog {
    async fn agents(&self) -> Result<Vec<acp_core::model::AgentDescriptor>, PortError> {
        Ok(Vec::new())
    }

    async fn profile(
        &self,
        _id: &AgentId,
    ) -> Result<Option<acp_core::model::AgentProfile>, PortError> {
        Ok(None)
    }

    async fn agent_capabilities(
        &self,
        _agent: &AgentRef,
    ) -> Result<acp_core::model::CapabilitySet, PortError> {
        Err(unavailable())
    }
}

/// 后端工厂：`session.start` 的端点在准备面建立，这条路径不应触碰它。
struct TestBackends;

#[async_trait]
impl SessionBackendFactory for TestBackends {
    async fn create(
        &self,
        _session: &SessionId,
        _request: CreateSessionRequest,
        _sink: acp_core::ports::EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Err(unavailable())
    }

    async fn open(
        &self,
        _reference: SessionReference,
        _sink: acp_core::ports::EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Err(unavailable())
    }

    async fn resume(
        &self,
        _session: &SessionId,
        _request: ResumeSessionRequest,
        _sink: acp_core::ports::EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Err(unavailable())
    }

    async fn terminate_agent(&self, _session: &SessionId) -> Result<(), PortError> {
        Ok(())
    }
}

struct NullPublisher;

impl EventPublisher for NullPublisher {
    fn publish(&self, _event: acp_core::model::CommittedEvent) {}
}

struct TestFiles;

#[async_trait]
impl acp_core::ports::SessionFiles for TestFiles {
    async fn list(
        &self,
        _root: &str,
        _path: Option<&acp_core::model::SessionFilePath>,
        _hide_ignored: bool,
        _after: Option<&acp_core::model::SessionPageToken>,
    ) -> Result<acp_core::model::SessionFilePage, PortError> {
        Err(unavailable())
    }

    async fn open_content(
        &self,
        _root: &str,
        _path: &acp_core::model::SessionFilePath,
        _purpose: acp_core::model::FileReadPurpose,
    ) -> Result<acp_core::model::ContentSnapshot, PortError> {
        Err(unavailable())
    }

    async fn read_content(
        &self,
        _version: &acp_core::model::ContentVersion,
        _offset: u64,
    ) -> Result<acp_core::model::SessionFileChunk, PortError> {
        Err(unavailable())
    }

    async fn release_content(
        &self,
        _version: &acp_core::model::ContentVersion,
    ) -> Result<(), PortError> {
        Ok(())
    }

    async fn git_status(
        &self,
        _root: &str,
        _after: Option<&acp_core::model::SessionPageToken>,
    ) -> Result<acp_core::model::GitStatusPage, PortError> {
        Err(unavailable())
    }

    async fn git_diff(
        &self,
        _root: &str,
        _path: &acp_core::model::SessionFilePath,
    ) -> Result<acp_core::model::GitDiff, PortError> {
        Err(unavailable())
    }
}

struct TestOwner;

impl OwnerReachability for TestOwner {
    fn reachable(&self, _owner: &acp_core::model::NodeId) -> bool {
        false
    }
}

/// 转正后的端点���`claim` 的返回值）：只数派发次数，并证明自己带着真实的 Agent 会话标识。
struct TestEndpoint {
    reference: SessionReference,
    agent_session: AgentSessionId,
    prompts: AtomicU32,
}

impl TestEndpoint {
    fn new(session: &SessionId) -> Self {
        Self {
            reference: SessionReference::Owned(OwnedSessionRef::new(session.clone())),
            agent_session: AgentSessionId::new("agent-session-1").expect("agent session"),
            prompts: AtomicU32::new(0),
        }
    }

    fn prompts(&self) -> u32 {
        self.prompts.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SessionEndpoint for TestEndpoint {
    fn reference(&self) -> SessionReference {
        self.reference.clone()
    }

    fn agent_session_id(&self) -> Option<&AgentSessionId> {
        Some(&self.agent_session)
    }

    async fn prompt(
        &self,
        _request: acp_core::model::PromptRequest,
        _at: acp_core::model::Timestamp,
    ) -> Result<TurnAccepted, PortError> {
        self.prompts.fetch_add(1, Ordering::SeqCst);
        Ok(TurnAccepted {
            turn: TurnId::new(&uuid_text()).expect("turn"),
        })
    }

    async fn validate_prompt(
        &self,
        _request: &acp_core::model::PromptRequest,
    ) -> Result<(), PortError> {
        Ok(())
    }

    async fn cancel(&self, _turn: Option<TurnId>) -> Result<(), PortError> {
        Ok(())
    }

    async fn set_mode(&self, _mode: &acp_core::model::ModeId) -> Result<(), PortError> {
        Ok(())
    }

    async fn list_config(&self) -> Result<Vec<ConfigOption>, PortError> {
        Ok(Vec::new())
    }

    async fn modes(&self) -> Result<ModeState, PortError> {
        Ok(ModeState::new(None, Vec::new()))
    }

    async fn set_config(&self, _id: &ConfigOptionId, _value: ConfigValue) -> Result<(), PortError> {
        Ok(())
    }

    async fn resolve_interaction(
        &self,
        _interaction: &acp_core::model::InteractionId,
        _resolution: acp_core::model::InteractionResolution,
    ) -> Result<(), PortError> {
        Ok(())
    }

    async fn read_history(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
        Err(unavailable())
    }

    async fn close(&self) -> Result<(), PortError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// 装配
// ---------------------------------------------------------------------------------------------

/// 一条已协商准备 feature 的 `session.start` 处理器，以及它背后的窄替身。
struct Start {
    fixture: fixture_tests::Fixture,
    route: Arc<CommandRoute>,
    registry: Arc<ConnectionRegistry>,
    lifecycle: Arc<LocalSessionLifecycle>,
    endpoint: Arc<TestEndpoint>,
    session: SessionId,
    generation: u64,
}

impl Start {
    /// 一条已注册、已协商 `core.session-preparation.v1` 的连接。
    ///
    /// 必须真的注册进连接表：命令面的 `reclaim` 会把「不在注册表里且不是当前连接」的连接期状态
    /// 整个丢掉，未注册的句柄在第二次请求时就会失去 feature 协商结果。
    fn connection(&self) -> (Arc<ConnectionHandle>, tokio::sync::mpsc::Receiver<Outbound>) {
        let (handle, receiver) = fixture_tests::handle(fixture_tests::DEVICE);
        self.registry.register(handle.clone());
        self.route
            .on_authenticated(&handle, &[PREPARATION_FEATURE.to_owned()]);
        (handle, receiver)
    }

    fn first(&self) -> &Arc<fixture_tests::TestFirst> {
        &self.fixture.first
    }

    fn host(&self) -> &Arc<fixture_tests::TestHost> {
        &self.fixture.host
    }

    /// 准备页仍以同一 ID 处于 `ready`（被拒后可重试的直接判据；是否解冻由重试是否成功证明）。
    fn preparation_is_ready(&self) -> bool {
        self.fixture
            .directory
            .view(&self.session)
            .is_some_and(|view| view.is_ready())
    }

    fn receipt(&self) -> sync_protocol::common::PreparationReceipt {
        fixture_tests::sign_receipt(
            &self.fixture,
            &self.session,
            fixture_tests::TAB,
            fixture_tests::AGENT,
        )
    }

    fn generation_text(&self) -> String {
        self.generation.to_string()
    }

    /// 准备好的端点选项：当前模式 `default`、配置 `model=gpt-5`（与载荷的选择一致）。
    fn selectable_options() -> preparation::PreparationOptions {
        let mode = ModeRef::try_new(
            acp_core::model::ModeId::new("default").expect("mode"),
            "Default",
        )
        .expect("mode ref");
        preparation::PreparationOptions {
            modes: ModeState::new(Some(mode.clone()), vec![mode]),
            config: vec![
                ConfigOption::try_new(
                    ConfigOptionId::new("model").expect("id"),
                    "Model",
                    None,
                    None,
                    acp_core::model::ConfigOptionKind::Select,
                    ConfigValue::select("gpt-5").expect("value"),
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
        }
    }
}

async fn start() -> Start {
    let fixture = fixture_tests::fixture();
    let (session, view) = fixture_tests::ready_preparation(&fixture).await;
    let workspace = fixture.trust.workspace().to_owned();

    let endpoint = Arc::new(TestEndpoint::new(&session));
    fixture.host.set_endpoint(endpoint.clone());
    fixture.host.set_options(Start::selectable_options());

    let first = fixture.first.clone();
    let store = Arc::new(TestStore::new(session.clone(), &workspace, first.clone()));
    let clock: Arc<dyn Clock> = Arc::new(FixedClock);
    let publisher: Arc<dyn EventPublisher> = Arc::new(NullPublisher);
    let store_port: Arc<dyn SessionStore> = store.clone();
    let lifecycle = Arc::new(LocalSessionLifecycle::new(
        clock.clone(),
        store_port.clone(),
        publisher.clone(),
        LifecycleConfig::new(60_000, 4).expect("lifecycle config"),
    ));
    lifecycle
        .converge_on_startup()
        .await
        .expect("启动收敛必须完成");

    let trust = Arc::new(TestCoreTrust::new());
    let broker = Arc::new(Broker::new(
        BrokerDeps {
            store: store_port.clone(),
            backends: Arc::new(TestBackends),
            trust: trust.clone() as Arc<dyn acp_core::ports::TrustStore>,
            publisher: publisher.clone(),
            clock: clock.clone(),
            ids: Arc::new(fixture_tests::TestIds),
            audit: None,
        },
        BrokerConfig::default(),
    ));
    assert!(
        broker.set_lifecycle(lifecycle.clone()),
        "生命周期只接线一次"
    );

    let config = Arc::new(TestConfig {
        record: DirectoryRecord::try_new(directory_id(&workspace), &workspace, at(), at())
            .expect("directory record"),
    });
    let core = Arc::new(
        UseCases::new(UseCaseDeps {
            broker: broker.clone(),
            store: store_port,
            trust,
            audit: Arc::new(TestAudit),
            config,
            attachments: Arc::new(TestAttachments),
            catalog: Arc::new(TestCatalog),
            clock: clock.clone(),
            ids: Arc::new(fixture_tests::TestIds),
        })
        .with_first_submission(first as Arc<dyn FirstSubmission>),
    );

    let authority = Arc::new(identity_auth::Authority::new(
        acp_core::model::NodeId::new(fixture_tests::NODE).expect("node"),
        KeyHandle::new("test-node-key").expect("key handle"),
        Arc::new(TestKeystore {
            public_key: test_public_key(),
        }),
        Arc::new(ZeroEntropy),
        clock,
    ));
    let registry = ConnectionRegistry::new();
    let route = Arc::new(
        CommandRoute::new(
            core,
            broker,
            authority,
            registry.clone(),
            Arc::new(TestOwner),
            Arc::new(TestFiles),
            None,
        )
        .with_preparation(
            fixture.preparations.clone(),
            fixture.host.clone() as Arc<dyn preparation::PreparationEndpoints>,
        ),
    );

    Start {
        fixture,
        route,
        registry,
        lifecycle,
        endpoint,
        session,
        generation: view.generation.get(),
    }
}

// ---------------------------------------------------------------------------------------------
// 帧构造与回包读取
// ---------------------------------------------------------------------------------------------

fn uuid_text() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn command_envelope(body: serde_json::Value) -> Envelope {
    let text = serde_json::to_string(&body).expect("json");
    let raw = serde_json::value::RawValue::from_string(text).expect("raw");
    Envelope::new(
        MessageType::Command,
        sync_protocol::common::Uuid::parse(&uuid_text()).expect("message id"),
        Some(ConnectionFields {
            connection_id: sync_protocol::common::Uuid::parse(&uuid_text()).expect("connection id"),
            connection_sequence: DecimalString::parse("1").expect("sequence"),
        }),
        raw,
    )
    .expect("envelope")
}

/// 一条 `kind: initial` 的 `session.start`（wire 形状与客户端逐字段一致）。
fn initial_body(
    start: &Start,
    request: &str,
    mode: Option<&str>,
    config: Option<serde_json::Value>,
) -> serde_json::Value {
    let receipt = serde_json::to_value(start.receipt()).expect("receipt json");
    let mut payload = serde_json::json!({
        "kind": "initial",
        "receipt": receipt,
        "runtimeEpoch": fixture_tests::RUNTIME,
        "generation": start.generation_text(),
        "content": [{"type": "text", "text": "first message"}],
    });
    if let Some(mode) = mode {
        payload["mode"] = serde_json::json!(mode);
    }
    if let Some(config) = config {
        payload["configOptions"] = config;
    }
    serde_json::json!({
        "requestId": request,
        "command": "session.start",
        "sessionId": start.session.as_str(),
        "payload": payload,
    })
}

fn continue_body(
    start: &Start,
    request: &str,
    first_submission: &str,
    retry_of: &str,
    mode: Option<&str>,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "kind": "continue",
        "firstSubmissionId": first_submission,
        "retryOfRequestId": retry_of,
    });
    if let Some(mode) = mode {
        payload["mode"] = serde_json::json!(mode);
    }
    serde_json::json!({
        "requestId": request,
        "command": "session.start",
        "sessionId": start.session.as_str(),
        "payload": payload,
    })
}

fn decode_error(outbound: &Outbound) -> sync_protocol::error::Body {
    let envelope = Envelope::decode(&outbound.text).expect("envelope");
    assert_eq!(
        envelope.message_type(),
        MessageType::Error,
        "expected an error frame, got {:?}",
        envelope.message_type()
    );
    serde_json::from_str(envelope.body().get()).expect("error body")
}

fn decode_result(outbound: &Outbound) -> CommandResult {
    let envelope = Envelope::decode(&outbound.text).expect("envelope");
    assert_eq!(
        envelope.message_type(),
        MessageType::CommandResult,
        "expected a command.result frame, got {:?}",
        envelope.message_type()
    );
    serde_json::from_str(envelope.body().get()).expect("result body")
}

/// 发出 `session.start`：`Ok(accepted/终态 result)`，拒绝（`command.result.status = rejected` 或
/// `error` 帧）作为 `Err(PublicError)` 返回——两者都是消费者可见的拒绝形状，但断言只看错误码。
async fn send(
    start: &Start,
    handle: &Arc<ConnectionHandle>,
    receiver: &mut tokio::sync::mpsc::Receiver<Outbound>,
    body: serde_json::Value,
) -> Result<CommandResult, sync_protocol::common::PublicError> {
    let outcome = start.route.route(handle, &command_envelope(body)).await;
    assert_eq!(outcome, crate::sync::conn::RouteOutcome::Claimed);
    let outbound = receiver.try_recv().expect("a reply frame");
    if Envelope::decode(&outbound.text)
        .expect("envelope")
        .message_type()
        == MessageType::Error
    {
        let body = decode_error(&outbound);
        return Err(sync_protocol::common::PublicError {
            code: body.code,
            message: body.message,
            retryable: body.retryable,
            details: body.details,
        });
    }
    let result = decode_result(&outbound);
    if result.status == CommandState::Rejected {
        return Err(result
            .error
            .as_ref()
            .expect("rejected carries an error")
            .clone());
    }
    Ok(result)
}

// ---------------------------------------------------------------------------------------------
// kind = initial
// ---------------------------------------------------------------------------------------------

/// 受理：一次提交、一次派发、回包是 `accepted`（不是 completed），并把准备名额转给正式会话。
///
/// 变红点：①受理前就回包；②准备条目仍然可观察（`finish_submission` 没跑）；③派发两次；
/// ④受理把准备名额当作「交还」而不是「转移」。
#[tokio::test]
async fn an_initial_start_commits_once_dispatches_once_and_registers_the_session() {
    let start = start().await;
    let (handle, mut receiver) = start.connection();
    assert_eq!(
        start.fixture.full.reserve_calls(),
        1,
        "准备分配预占一次名额"
    );

    let request = uuid_text();
    let result = send(
        &start,
        &handle,
        &mut receiver,
        initial_body(
            &start,
            &request,
            Some("default"),
            Some(serde_json::json!({"model": "gpt-5"})),
        ),
    )
    .await
    .expect("a valid initial start is accepted");

    assert_eq!(result.status, CommandState::Accepted, "受理不是完成");
    assert_eq!(result.request_id.as_str(), request);
    assert_eq!(result.command, CommandName::SessionStart);
    assert_eq!(
        start.host().configure_calls(),
        1,
        "首次提交前必须在当前资源代上复核草稿选择"
    );

    // 提交与派发各一次；首条只有一份。
    assert_eq!(start.first().commits(), 1, "首条受理事务只提交一次");
    assert_eq!(start.first().dispatches(), 1, "派发围栏只跨越一次");
    assert_eq!(start.endpoint.prompts(), 1, "Agent 只收到一次 prompt");
    let state = start.first().state().lock().expect("lock");
    assert_eq!(state.messages.len(), 1, "持久首条用户消息只有一份");
    let record = state
        .records
        .get(start.session.as_str())
        .expect("首条关联已持久化")
        .clone();
    assert_eq!(record.stage, FirstSubmissionStage::DispatchStarted);
    assert!(record.ever_dispatched);
    assert_eq!(
        record.first_request_id.as_str(),
        request,
        "首条关联绑定本次请求"
    );
    drop(state);

    // 准备条目已从目录移出（名额转给正式会话，不是交还）。
    assert!(
        start.fixture.directory.view(&start.session).is_none(),
        "已受理的准备不再有可观察的运行期资源"
    );
    assert_eq!(start.fixture.directory.occupied(), 0);
    assert_eq!(
        start.fixture.full.release_calls(),
        0,
        "转移给正式会话的名额不得被当作释放"
    );

    // 正式会话在生命周期里是 ready：后续激活只复用、不再建立，也会参与保留期/容量回收。
    let plan = start
        .lifecycle
        .activate_session(
            &Actor::LocalCli,
            ActivationRequest {
                actor: Actor::LocalCli,
                request_id: RequestId::new(&uuid_text()).expect("request"),
                request_fingerprint: Digest::new(&"A".repeat(43)).expect("digest"),
                session: start.session.clone(),
            },
        )
        .await
        .expect("激活可判定");
    assert!(
        matches!(plan, ActivationPlan::Reused { .. }),
        "已提交的会话必须是 ready 资源（实际 {plan:?}）"
    );
}

/// 同 `requestId` 重放只读原受理：不再提交、不再派发，回包沿用首次的 `acceptedAt`。
#[tokio::test]
async fn replaying_the_same_request_reads_the_original_receipt() {
    let start = start().await;
    let (handle, mut receiver) = start.connection();
    let request = uuid_text();
    let body = initial_body(&start, &request, None, None);

    let first = send(&start, &handle, &mut receiver, body.clone())
        .await
        .expect("first accept");
    let replay = send(&start, &handle, &mut receiver, body)
        .await
        .expect("replay reads the original receipt");

    assert_eq!(replay.status, CommandState::Accepted);
    assert_eq!(
        replay.accepted_at.as_ref().map(|at| at.as_str()),
        first.accepted_at.as_ref().map(|at| at.as_str()),
        "重放必须沿用首次落盘的 acceptedAt"
    );
    assert_eq!(start.first().commits(), 1, "重放不得第二次提交首条");
    assert_eq!(start.first().dispatches(), 1, "重放不得第二次派发");
    assert_eq!(start.endpoint.prompts(), 1);
}

/// 原子保存失败：不得派发、不得留下半提交，且准备页仍以同一 ID 可重试。
#[tokio::test]
async fn a_failed_first_submission_commit_dispatches_nothing_and_keeps_the_preparation() {
    let start = start().await;
    let (handle, mut receiver) = start.connection();
    start.first().fail_next_commit(UnavailableKind::IoError);

    let error = send(
        &start,
        &handle,
        &mut receiver,
        initial_body(&start, &uuid_text(), None, None),
    )
    .await
    .expect_err("a failed commit must not be accepted");
    assert_eq!(error.code, ErrorCode::InternalUnavailable);

    assert_eq!(start.first().dispatches(), 0, "保存失败不得跨越派发围栏");
    assert_eq!(start.endpoint.prompts(), 0, "保存失败不得派发 prompt");
    assert!(start.preparation_is_ready(), "冻结必须被撤销");
    assert_eq!(
        start.host().releases().len(),
        1,
        "本次转正失败的临时端点必须按资源代释放"
    );

    // 同一准备页可以重试（新的请求 ID）：不是 suspended 的 `session.busy`。
    let retry = send(
        &start,
        &handle,
        &mut receiver,
        initial_body(&start, &uuid_text(), None, None),
    )
    .await
    .expect("retry after a failed commit is accepted");
    assert_eq!(retry.status, CommandState::Accepted);
    assert_eq!(start.first().commits(), 2, "失败的那次没有留下受理记录");
    assert_eq!(start.endpoint.prompts(), 1);
}

/// 选择被 Agent 拒绝（此处为不可用的模式）：撤销冻结并释放端点，随后用有效选择重试必须成功。
///
/// 变红点：①只回错误、不解冻——重试落在 `session.busy`；②不释放临时端点（容量被永久占住）。
#[tokio::test]
async fn an_unavailable_selection_unwinds_the_freeze_and_a_retry_succeeds() {
    let start = start().await;
    let (handle, mut receiver) = start.connection();

    let error = send(
        &start,
        &handle,
        &mut receiver,
        initial_body(&start, &uuid_text(), Some("plan"), None),
    )
    .await
    .expect_err("an unavailable mode is rejected");
    assert_eq!(error.code, ErrorCode::CapabilityUnsupportedByAgent);
    assert_eq!(
        start.host().releases(),
        vec![(start.session.as_str().to_owned(), start.generation)],
        "被拒的选择必须释放该资源代的临时端点"
    );
    assert_eq!(start.first().dispatches(), 0, "被拒的选择不得派发");
    assert!(start.preparation_is_ready(), "冻结必须被撤销");
    assert_eq!(
        start.fixture.full.release_calls(),
        0,
        "同一准备页要继续可重试，名额不得被交还"
    );

    // 同 ID 重试（新的请求 ID）必须受理，而不是 `session.busy`。
    let retry = send(
        &start,
        &handle,
        &mut receiver,
        initial_body(&start, &uuid_text(), Some("default"), None),
    )
    .await
    .expect("the retry is accepted");
    assert_eq!(retry.status, CommandState::Accepted);
    assert_eq!(start.first().commits(), 1);
    assert_eq!(start.endpoint.prompts(), 1);
    assert_eq!(start.host().releases().len(), 1, "重试不再释放端点");
}

// ---------------------------------------------------------------------------------------------
// kind = continue
// ---------------------------------------------------------------------------------------------

/// 一次「可证明未派发」的失败尝试（`saved_not_dispatched` + `Failed` 终态）。
fn failed_attempt(start: &Start, failed_request: &RequestId) -> FirstSubmissionRecord {
    FirstSubmissionRecord {
        session: start.session.clone(),
        first_submission_id: acp_core::model::FirstSubmissionId::new(&uuid_text())
            .expect("first submission"),
        user_message_id: acp_core::model::MessageId::new(&uuid_text()).expect("message"),
        first_request_id: failed_request.clone(),
        current_request_id: failed_request.clone(),
        stage: FirstSubmissionStage::SavedNotDispatched,
        ever_dispatched: false,
        agent_session: None,
        resource_generation: Some(ResourceGeneration::new(1)),
        mode_id: None,
        config: ViewJson::empty_object(),
        input_digest: Digest::new(&"A".repeat(43)).expect("digest"),
        terminal: Some(acp_core::model::RequestTerminal {
            request_id: failed_request.clone(),
            outcome: RequestOutcome::Failed,
            at: at(),
            error: Some(
                PublicError::coded(
                    "internal.unavailable",
                    "the agent could not be established",
                    true,
                )
                .expect("error"),
            ),
        }),
    }
}

/// 种子：首条已受理、首个尝试可证明未派发地失败，正文仍可读（继续的正文来源）。
fn seed_failed_attempt(start: &Start, failed_request: &RequestId) -> FirstSubmissionRecord {
    let record = failed_attempt(start, failed_request);
    start.first().set(record.clone());
    start.first().state().lock().expect("lock").messages.insert(
        start.session.as_str().to_owned(),
        acp_core::model::FirstUserMessage {
            session: start.session.clone(),
            message: record.user_message_id.clone(),
            turn: Some(TurnId::new(&uuid_text()).expect("turn")),
            content: acp_core::model::JsonValueText::new(
                r#"[{"type":"text","text":"first message"}]"#,
            )
            .expect("content"),
            created_at: at(),
        },
    );
    record
}

/// 首个提交已有**成功**终态：继续必须回 `command.idempotency_conflict`（指向原终态），
/// 既不是 `uncertain`，也不建端点/不派发。
#[tokio::test]
async fn continue_after_a_successful_terminal_is_an_idempotency_conflict() {
    let start = start().await;
    let (handle, mut receiver) = start.connection();
    let dispatched = RequestId::new(&uuid_text()).expect("request");
    let mut record = failed_attempt(&start, &dispatched);
    record.stage = FirstSubmissionStage::Terminal;
    record.ever_dispatched = true;
    record.terminal.as_mut().expect("terminal").outcome = RequestOutcome::Completed;
    start.first().set(record.clone());

    let error = send(
        &start,
        &handle,
        &mut receiver,
        continue_body(
            &start,
            &uuid_text(),
            record.first_submission_id.as_str(),
            dispatched.as_str(),
            None,
        ),
    )
    .await
    .expect_err("a successful first submission cannot be continued");
    assert_eq!(error.code, ErrorCode::CommandIdempotencyConflict);
    assert_eq!(start.host().establish_calls(), 0, "不得重建端点");
    assert_eq!(start.first().dispatches(), 0);
}

/// 已跨越派发围栏（`ever_dispatched` / `dispatch_started`）：继续必须是 `command.uncertain`。
#[tokio::test]
async fn continue_after_the_dispatch_fence_is_uncertain() {
    let start = start().await;
    let (handle, mut receiver) = start.connection();
    let dispatched = RequestId::new(&uuid_text()).expect("request");
    let mut record = failed_attempt(&start, &dispatched);
    record.stage = FirstSubmissionStage::DispatchStarted;
    record.ever_dispatched = true;
    record.terminal = None;
    start.first().set(record.clone());

    let error = send(
        &start,
        &handle,
        &mut receiver,
        continue_body(
            &start,
            &uuid_text(),
            record.first_submission_id.as_str(),
            dispatched.as_str(),
            None,
        ),
    )
    .await
    .expect_err("a possibly-dispatched attempt cannot be continued");
    assert_eq!(error.code, ErrorCode::CommandUncertain);
    assert_eq!(start.host().establish_calls(), 0, "uncertain 不重建端点");
    assert_eq!(start.first().dispatches(), 0);
}

/// 可证明未派发的失败 + 匹配的 `retryOfRequestId`：以新请求 ID 受理，正文只有一份，派发一次。
#[tokio::test]
async fn continue_after_a_provable_failure_dispatches_the_first_message_once() {
    let start = start().await;
    let (handle, mut receiver) = start.connection();
    let failed = RequestId::new(&uuid_text()).expect("request");
    let record = seed_failed_attempt(&start, &failed);
    let retry = uuid_text();

    let result = send(
        &start,
        &handle,
        &mut receiver,
        continue_body(
            &start,
            &retry,
            record.first_submission_id.as_str(),
            failed.as_str(),
            None,
        ),
    )
    .await
    .expect("a provably-not-dispatched failure can be continued");
    assert_eq!(result.status, CommandState::Accepted);
    assert_eq!(result.request_id.as_str(), retry);

    assert_eq!(start.host().establish_calls(), 1, "继续重建临时端点一次");
    assert_eq!(start.host().claim_calls(), 1);
    assert_eq!(start.first().dispatches(), 1, "只跨越一次派发围栏");
    assert_eq!(start.endpoint.prompts(), 1, "Agent 只收到一次 prompt");
    assert_eq!(start.first().commits(), 0, "继续不得再提交一次首条受理");
    let state = start.first().state().lock().expect("lock");
    assert_eq!(state.messages.len(), 1, "首条用户消息仍只有一份");
    let updated = state
        .records
        .get(start.session.as_str())
        .expect("首条关联")
        .clone();
    assert_eq!(updated.stage, FirstSubmissionStage::DispatchStarted);
    assert_eq!(
        updated.current_request_id.as_str(),
        retry,
        "当前尝试换成新的请求 ID"
    );
    assert_eq!(updated.first_request_id, failed, "首个请求标识不被改写");
    drop(state);

    // 同一 requestId 再次到达：只读本次尝试的受理，不再派发。
    let replay = send(
        &start,
        &handle,
        &mut receiver,
        continue_body(
            &start,
            &retry,
            record.first_submission_id.as_str(),
            failed.as_str(),
            None,
        ),
    )
    .await
    .expect("replay reads the attempt receipt");
    assert_eq!(replay.status, CommandState::Accepted);
    assert_eq!(start.first().dispatches(), 1);
    assert_eq!(start.endpoint.prompts(), 1);
}

/// 继续的选择复核失败：写本次尝试的 `failed` 终态并释放刚建立的端点；用有效选择换个请求 ID
/// 仍可继续（不是永久卡死）。
#[tokio::test]
async fn continue_with_an_unavailable_selection_releases_the_new_endpoint() {
    let start = start().await;
    let (handle, mut receiver) = start.connection();
    let failed = RequestId::new(&uuid_text()).expect("request");
    let record = seed_failed_attempt(&start, &failed);
    let rejected_request = uuid_text();

    let error = send(
        &start,
        &handle,
        &mut receiver,
        continue_body(
            &start,
            &rejected_request,
            record.first_submission_id.as_str(),
            failed.as_str(),
            Some("plan"),
        ),
    )
    .await
    .expect_err("an unavailable mode is rejected");
    assert_eq!(error.code, ErrorCode::CapabilityUnsupportedByAgent);
    assert_eq!(start.host().establish_calls(), 1);
    assert_eq!(start.host().claim_calls(), 0, "复核失败不得转正端点");
    assert_eq!(start.host().releases().len(), 1, "刚建立的临时端点必须释放");
    assert_eq!(start.first().settlements(), 1, "本次尝试写下 failed 终态");
    assert_eq!(start.first().dispatches(), 0);
    assert_eq!(start.endpoint.prompts(), 0);

    // 终态为 failed 的新尝试可以再次显式继续（新的请求 ID）。
    let retry = send(
        &start,
        &handle,
        &mut receiver,
        continue_body(
            &start,
            &uuid_text(),
            record.first_submission_id.as_str(),
            &rejected_request,
            None,
        ),
    )
    .await
    .expect("a failed attempt can be continued again");
    assert_eq!(retry.status, CommandState::Accepted);
    assert_eq!(start.first().dispatches(), 1);
    assert_eq!(start.endpoint.prompts(), 1);
}

/// 两个不同的继续请求并发竞争同一首条：阶段 CAS 只允许一次派发，另一个明确冲突。
#[tokio::test]
async fn concurrent_continues_dispatch_at_most_once() {
    let start = Arc::new(start().await);
    let failed = RequestId::new(&uuid_text()).expect("request");
    let record = seed_failed_attempt(&start, &failed);

    let (first_handle, mut first_receiver) = start.connection();
    let (second_handle, mut second_receiver) = start.connection();
    let first_request = uuid_text();
    let second_request = uuid_text();
    let first_body = continue_body(
        &start,
        &first_request,
        record.first_submission_id.as_str(),
        failed.as_str(),
        None,
    );
    let second_body = continue_body(
        &start,
        &second_request,
        record.first_submission_id.as_str(),
        failed.as_str(),
        None,
    );

    let (first, second) = tokio::join!(
        send(&start, &first_handle, &mut first_receiver, first_body),
        send(&start, &second_handle, &mut second_receiver, second_body),
    );

    let outcomes = [first, second];
    let accepted = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, Ok(result) if result.status == CommandState::Accepted))
        .count();
    assert_eq!(accepted, 1, "并发继续最多受理一个: {outcomes:?}");
    let rejected = outcomes
        .iter()
        .filter(|outcome| {
            matches!(
                outcome,
                Err(error) if error.code == ErrorCode::CommandUncertain
                    || error.code == ErrorCode::CommandIdempotencyConflict
            )
        })
        .count();
    assert_eq!(
        rejected, 1,
        "另一个必须是明确的冲突/uncertain: {outcomes:?}"
    );

    assert_eq!(start.first().dispatches(), 1, "派发围栏只跨越一次");
    assert_eq!(start.endpoint.prompts(), 1, "Agent 只收到一次 prompt");
}
