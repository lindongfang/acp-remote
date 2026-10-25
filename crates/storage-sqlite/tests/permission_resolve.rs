//! `permission.resolve` 的**真实存储**端到端：Broker + `SqliteStore`（`InteractionAuthority` 也由
//! 真实存储实现），而不是只靠 fake。
//!
//! 它钉住三件事：
//!
//! - 候选项来自创建交互的事件 payload（`SqlReadView` 的旧投影 options 恒为空，真实路径不能依赖它）；
//! - 版本/类别/表单/上游校验发生在受理之前，失败不留下没有终态的 `accepted` 行；
//! - 失效请求无法经命令路径复活。

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use acp_core::broker::{Broker, BrokerConfig, BrokerDeps};
use acp_core::model::{
    Actor, AgentId, AgentRef, AgentSessionId, ClientCommand, CommandKind, CommandPayload,
    CommandReceipt, CommittedEvent, ConfigOption, ConfigOptionId, ConfigValue,
    CreateSessionRequest, DeviceId, Digest, EventKind, EventOrigin, EventPayload, EventType,
    InteractionId, InteractionKind, InteractionResolution, MessageId, ModeId, ModeState,
    OriginEpoch, PairingId, PendingEvent, PendingInteraction, PortError, PromptRequest, RequestId,
    ResumeSessionRequest, SessionId, SessionReference, StoredPolicy, Timestamp, TurnId,
    UnavailableKind, Version, ViewJson,
};
use acp_core::ports::{
    Clock, EventPublisher, EventSink, HistoryPage, HistoryQuery, IdGenerator, InteractionAuthority,
    NewSession, OwnedCommit, PendingInteractionWrite, SessionBackendFactory, SessionEndpoint,
    SessionStore, StateChange, TurnAccepted,
};
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::*;

const EPOCH: &str = "6f2a1f8e-7b1c-4a8f-9b0d-1c2d3e4f5a6b";
const DEVICE: &str = "55555555-5555-4555-8555-555555555555";
const INTERACTION: &str = "44444444-4444-4444-8444-444444444444";

fn at(hour: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T{hour:02}:00:00.000Z")).expect("timestamp")
}

fn uuid_text(n: u64) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

fn actor() -> Actor {
    Actor::Device {
        device: DeviceId::new(DEVICE).expect("device"),
    }
}

fn digest(seed: &str) -> Digest {
    Digest::new(&digest_text(seed)).expect("digest")
}

/// 固定时钟：broker 不读系统时间，终态与事件时间因此可预测。
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        at(2)
    }
}

/// 递增 id 生成器（只覆盖本用例真正会分配的几类）。
struct Ids {
    next: AtomicU64,
}

impl Ids {
    fn take(&self) -> u64 {
        self.next.fetch_add(1, Ordering::SeqCst)
    }
}

impl IdGenerator for Ids {
    fn session_id(&self) -> SessionId {
        SessionId::new(&uuid_text(100 + self.take())).expect("session id")
    }

    fn turn_id(&self) -> TurnId {
        TurnId::new(&uuid_text(200 + self.take())).expect("turn id")
    }

    fn interaction_id(&self) -> InteractionId {
        InteractionId::new(&uuid_text(300 + self.take())).expect("interaction id")
    }

    fn pairing_id(&self) -> PairingId {
        PairingId::new(&uuid_text(400 + self.take())).expect("pairing id")
    }

    fn message_id(&self) -> MessageId {
        MessageId::new(&uuid_text(500 + self.take())).expect("message id")
    }

    fn origin_epoch(&self) -> OriginEpoch {
        OriginEpoch::new(EPOCH).expect("epoch")
    }

    fn request_id(&self) -> RequestId {
        RequestId::new(&uuid_text(600 + self.take())).expect("request id")
    }
}

#[derive(Default)]
struct Published {
    seen: std::sync::Mutex<Vec<CommittedEvent>>,
}

impl EventPublisher for Published {
    fn publish(&self, event: CommittedEvent) {
        self.seen.lock().expect("published").push(event);
    }
}

/// 端点替身：记录后端真正收到的交互解析。
struct FakeEndpoint {
    reference: SessionReference,
    resolved: std::sync::Mutex<Vec<(InteractionId, InteractionResolution)>>,
}

#[async_trait::async_trait]
impl SessionEndpoint for FakeEndpoint {
    fn reference(&self) -> SessionReference {
        self.reference.clone()
    }

    fn agent_session_id(&self) -> Option<&AgentSessionId> {
        None
    }

    async fn prompt(
        &self,
        _request: PromptRequest,
        _at: Timestamp,
    ) -> Result<TurnAccepted, PortError> {
        Err(PortError::Unavailable(UnavailableKind::BackendUnsupported))
    }

    async fn validate_prompt(&self, _request: &PromptRequest) -> Result<(), PortError> {
        Ok(())
    }

    async fn cancel(&self, _turn: Option<TurnId>) -> Result<(), PortError> {
        Ok(())
    }

    async fn set_mode(&self, _mode: &ModeId) -> Result<(), PortError> {
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
        interaction: &InteractionId,
        resolution: InteractionResolution,
    ) -> Result<(), PortError> {
        self.resolved
            .lock()
            .expect("resolved")
            .push((interaction.clone(), resolution));
        Ok(())
    }

    async fn read_history(&self, _query: HistoryQuery) -> Result<HistoryPage, PortError> {
        Err(PortError::Unavailable(UnavailableKind::BackendUnsupported))
    }

    async fn close(&self) -> Result<(), PortError> {
        Ok(())
    }
}

struct Endpoints {
    opened: std::sync::Mutex<Vec<Arc<FakeEndpoint>>>,
}

#[async_trait::async_trait]
impl SessionBackendFactory for Endpoints {
    async fn create(
        &self,
        _session: &SessionId,
        _request: CreateSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Err(PortError::Unavailable(UnavailableKind::BackendUnsupported))
    }

    async fn open(
        &self,
        reference: SessionReference,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        let endpoint = Arc::new(FakeEndpoint {
            reference,
            resolved: std::sync::Mutex::new(Vec::new()),
        });
        self.opened.lock().expect("opened").push(endpoint.clone());
        Ok(Box::new(EndpointHandle(endpoint)))
    }

    async fn resume(
        &self,
        _session: &SessionId,
        _request: ResumeSessionRequest,
        _sink: EventSink,
    ) -> Result<Box<dyn SessionEndpoint>, PortError> {
        Err(PortError::Unavailable(UnavailableKind::BackendUnsupported))
    }

    async fn terminate_agent(&self, _session: &SessionId) -> Result<(), PortError> {
        Ok(())
    }
}

/// `Box<dyn SessionEndpoint>` 需要一个拥有所有权的小包装（测试里共享同一个可观察端点）。
struct EndpointHandle(Arc<FakeEndpoint>);

#[async_trait::async_trait]
impl SessionEndpoint for EndpointHandle {
    fn reference(&self) -> SessionReference {
        self.0.reference()
    }

    fn agent_session_id(&self) -> Option<&AgentSessionId> {
        None
    }

    async fn prompt(
        &self,
        request: PromptRequest,
        at: Timestamp,
    ) -> Result<TurnAccepted, PortError> {
        self.0.prompt(request, at).await
    }

    async fn validate_prompt(&self, _request: &PromptRequest) -> Result<(), PortError> {
        Ok(())
    }

    async fn cancel(&self, turn: Option<TurnId>) -> Result<(), PortError> {
        self.0.cancel(turn).await
    }

    async fn set_mode(&self, mode: &ModeId) -> Result<(), PortError> {
        self.0.set_mode(mode).await
    }

    async fn list_config(&self) -> Result<Vec<ConfigOption>, PortError> {
        self.0.list_config().await
    }

    async fn modes(&self) -> Result<ModeState, PortError> {
        self.0.modes().await
    }

    async fn set_config(&self, id: &ConfigOptionId, value: ConfigValue) -> Result<(), PortError> {
        self.0.set_config(id, value).await
    }

    async fn resolve_interaction(
        &self,
        interaction: &InteractionId,
        resolution: InteractionResolution,
    ) -> Result<(), PortError> {
        self.0.resolve_interaction(interaction, resolution).await
    }

    async fn read_history(&self, query: HistoryQuery) -> Result<HistoryPage, PortError> {
        self.0.read_history(query).await
    }

    async fn close(&self) -> Result<(), PortError> {
        self.0.close().await
    }
}

fn interaction_event(interaction: &InteractionId) -> PendingEvent {
    let view = format!(
        r#"{{"interactionId":"{interaction}","options":[{{"optionId":"allow-once","label":"Allow once","kind":"allow_once"}},{{"optionId":"reject-once","label":"Reject once","kind":"reject_once"}}]}}"#
    );
    PendingEvent::new(
        EventKind::Interaction,
        EventType::new("session.request_permission").expect("event type"),
        StoredPolicy::Durable,
        EventPayload::new(ViewJson::new(&view).expect("view"), None),
        EventOrigin::Agent,
        None,
        None,
    )
}

struct Harness {
    broker: Broker,
    store: Arc<SqliteStore>,
    session: SessionId,
    interaction: InteractionId,
    endpoints: Arc<Endpoints>,
    published: Arc<Published>,
}

async fn create_session(store: &SqliteStore) -> SessionId {
    store
        .commit(OwnedCommit {
            session: None,
            at: at(0),
            expected_version: None,
            state: Some(StateChange::Create(NewSession {
                title: Some("permission".to_owned()),
                agent: AgentRef::try_new(
                    AgentId::new("probe-agent").expect("agent id"),
                    "Probe Agent",
                )
                .expect("agent ref"),
            })),
            turns: Vec::new(),
            events: Vec::new(),
            interactions: Vec::new(),
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(OriginEpoch::new(EPOCH).expect("epoch")),
            input: None,
        })
        .await
        .expect("create session")
        .session_id
        .expect("session id")
}

async fn harness(dir: &std::path::Path) -> Harness {
    let store = Arc::new(
        SqliteStore::open(StorageConfig::new(dir), &at(0))
            .await
            .expect("open store"),
    );
    let session = create_session(&store).await;
    let interaction = InteractionId::new(INTERACTION).expect("interaction");
    let pending = PendingInteraction::try_new(
        interaction.clone(),
        InteractionKind::Permission,
        session.clone(),
        at(1),
        Vec::new(),
    )
    .expect("pending");
    store
        .commit(OwnedCommit {
            session: Some(session.clone()),
            at: at(1),
            expected_version: None,
            state: None,
            turns: Vec::new(),
            events: vec![interaction_event(&interaction)],
            interactions: vec![PendingInteractionWrite {
                interaction: pending,
                turn: None,
            }],
            compacted: Vec::new(),
            idempotency: None,
            command_terminal: None,
            origin_epoch: Some(OriginEpoch::new(EPOCH).expect("epoch")),
            input: None,
        })
        .await
        .expect("create interaction");

    let endpoints = Arc::new(Endpoints {
        opened: std::sync::Mutex::new(Vec::new()),
    });
    let published = Arc::new(Published::default());
    let broker = Broker::new(
        BrokerDeps {
            store: store.clone(),
            backends: endpoints.clone(),
            trust: store.clone(),
            publisher: published.clone(),
            clock: Arc::new(FixedClock),
            ids: Arc::new(Ids {
                next: AtomicU64::new(0),
            }),
            audit: None,
        },
        BrokerConfig::default(),
    )
    .with_interaction_authority(store.clone() as Arc<dyn InteractionAuthority>);
    Harness {
        broker,
        store,
        session,
        interaction,
        endpoints,
        published,
    }
}

fn command(
    request: u64,
    session: &SessionId,
    interaction: &InteractionId,
    option: &str,
) -> ClientCommand {
    ClientCommand {
        actor: actor(),
        request: RequestId::new(&uuid_text(700 + request)).expect("request"),
        command: "permission.resolve".to_owned(),
        kind: CommandKind::Mutation,
        session: Some(session.clone()),
        expected_version: None,
        request_fingerprint: digest("permission-resolve"),
        payload: CommandPayload::PermissionResolve {
            interaction: interaction.clone(),
            option_id: option.to_owned(),
        },
    }
}

/// 真实存储：options 来自事件 payload（不是空投影），一次解析同时落库并送达后端。
#[tokio::test]
async fn permission_resolve_reaches_the_authority_on_a_real_store() {
    let dir = temp_dir("permission-resolve-e2e");
    let harness = harness(&dir).await;

    let receipt = harness
        .broker
        .submit_mutation_versioned(
            &actor(),
            &command(1, &harness.session, &harness.interaction, "allow-once"),
            Some(Version::new(1)),
        )
        .await
        .expect("submit");
    assert!(matches!(receipt, CommandReceipt::Accepted { .. }));

    // 请求已被消费；后端收到的是 allow_once 判定（不是「缺失 option」的拒绝）。
    assert!(
        harness
            .store
            .effective(&harness.session)
            .await
            .expect("effective")
            .is_empty()
    );
    let opened = harness.endpoints.opened.lock().expect("opened").clone();
    assert_eq!(opened.len(), 1, "受理路径必须打开端点");
    let resolved = opened[0].resolved.lock().expect("resolved").clone();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].0, harness.interaction);
    assert!(matches!(
        &resolved[0].1,
        InteractionResolution::Permission(decision)
            if decision.option_id() == "allow-once"
    ));

    // 重试按幂等返回 accepted，不第二次解析。
    let retry = harness
        .broker
        .submit_mutation_versioned(
            &actor(),
            &command(1, &harness.session, &harness.interaction, "allow-once"),
            Some(Version::new(1)),
        )
        .await
        .expect("retry");
    assert!(matches!(retry, CommandReceipt::Accepted { .. }));
    assert_eq!(
        opened[0].resolved.lock().expect("resolved").len(),
        1,
        "重试不得二次派发解析"
    );
    assert!(!harness.published.seen.lock().expect("published").is_empty());
    drop(harness);
}

/// 版本不符在受理之前被拒绝：不留下没有终态的 `accepted` 行，同一 requestId 用正确版本仍可重试。
#[tokio::test]
async fn stale_version_is_rejected_before_acceptance_and_can_be_retried() {
    let dir = temp_dir("permission-resolve-stale");
    let harness = harness(&dir).await;

    let error = harness
        .broker
        .submit_mutation_versioned(
            &actor(),
            &command(1, &harness.session, &harness.interaction, "allow-once"),
            Some(Version::new(2)),
        )
        .await
        .expect_err("stale version");
    assert!(
        matches!(
            error,
            PortError::Conflict(acp_core::model::ConflictKind::VersionMismatch)
        ),
        "{error:?}"
    );
    // 请求未被消费，命令行也没落库（幂等键可被同一 requestId 重新使用）。
    assert_eq!(
        harness
            .store
            .effective(&harness.session)
            .await
            .expect("effective")
            .len(),
        1
    );
    assert!(
        harness
            .store
            .find_request(&RequestId::new(&uuid_text(701)).expect("request"), &actor())
            .await
            .expect("find")
            .is_none(),
        "校验失败不得留下 accepted 行"
    );

    let receipt = harness
        .broker
        .submit_mutation_versioned(
            &actor(),
            &command(1, &harness.session, &harness.interaction, "allow-once"),
            Some(Version::new(1)),
        )
        .await
        .expect("retry with the current version");
    assert!(matches!(receipt, CommandReceipt::Accepted { .. }));
    assert!(
        harness
            .store
            .effective(&harness.session)
            .await
            .expect("effective")
            .is_empty()
    );
    drop(harness);
}

/// 已失效（上游丢失）的请求不能经真实 `permission.resolve` 复活：命令被拒绝为「不存在」。
#[tokio::test]
async fn invalidated_requests_are_not_resolvable_through_the_command_path() {
    let dir = temp_dir("permission-resolve-invalidated");
    let harness = harness(&dir).await;
    assert_eq!(
        harness
            .store
            .invalidate_pending(Some(&harness.session), at(3))
            .await
            .expect("invalidate"),
        1
    );
    let receipt = harness
        .broker
        .submit_mutation_versioned(
            &actor(),
            &command(1, &harness.session, &harness.interaction, "allow-once"),
            Some(Version::new(1)),
        )
        .await
        .expect("rejected receipt");
    match receipt {
        CommandReceipt::Rejected { error } => {
            assert_eq!(error.code(), "command.not_found");
        }
        other => panic!("expected a rejected receipt, got {other:?}"),
    }
    assert!(
        harness
            .store
            .effective(&harness.session)
            .await
            .expect("effective")
            .is_empty()
    );
    drop(harness);
}
