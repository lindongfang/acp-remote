//! Independent test-only trusted ports. SQLite is always delegated, never replaced.
use acp_remote::{
    application::{
        queue_owner::{
            NativeQueueDriver, QueueEnvironmentPort, QueueOwnerEvent, QueueRead, QueueService,
        },
        request_registry::{
            AuditFailure, AuditMetadata, AuditPort, BeginRecord, ConsumeLease, CurrentRequestActor,
            NodeClock, OperationContext, OperationProbe, QueueAction, QueueOutcome,
            RegisteredIntent, RegistrationFact, RegistrationId, RegistryDeadline, RegistryError,
            RequestAuthority, RequestFact, RequestRegistryRepository, ValidatedBodyPort,
            ValidatedMutation,
        },
    },
    domain::{
        ActorContext, EntityId,
        input_queue::DispatchReadiness,
        native::{
            HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch,
        },
    },
    session_storage::request_registry::SqliteRequestRegistry,
    storage::{NodeMeta, SCHEMA_VERSION, Storage},
};
use sha2::{Digest, Sha256};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
    },
};
use tokio::{
    sync::Notify,
    time::{Duration, Instant, timeout},
};

pub const fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
pub const T1: EntityId = id(40);
pub const T2: EntityId = id(41);
pub const fn resource(load: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(id(101)),
        connection: HarnessConnectionEpoch(id(102)),
        load: LoadEpoch(id(load)),
    }
}
pub fn native(source: u8) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(id(source)), "reg-q3-synthetic").unwrap()
}
pub const fn deadline() -> Duration {
    Duration::from_secs(20)
}
pub fn closing_deadline() -> RegistryDeadline {
    RegistryDeadline::new(Instant::now() + deadline())
}

#[derive(Clone, Copy)]
pub struct Actor {
    pub id: EntityId,
    pub generation: i64,
}
pub const A: Actor = Actor {
    id: id(10),
    generation: 1,
};
pub const B: Actor = Actor {
    id: id(11),
    generation: 2,
};
pub const REVOKED: Actor = Actor {
    id: id(12),
    generation: 3,
};

#[derive(Clone)]
pub struct Authority {
    pub targets: Arc<RwLock<HashMap<EntityId, NativeSessionKey>>>,
    pub deny_write: Arc<AtomicBool>,
    pub deny_read: Arc<AtomicBool>,
    pub write_calls: Arc<AtomicU64>,
}
impl Authority {
    pub fn new() -> Self {
        Self {
            targets: Arc::new(RwLock::new(HashMap::from([
                (T1, native(1)),
                (T2, native(2)),
            ]))),
            deny_write: Arc::new(AtomicBool::new(false)),
            deny_read: Arc::new(AtomicBool::new(false)),
            write_calls: Arc::new(AtomicU64::new(0)),
        }
    }
}
#[allow(
    clippy::unused_async_trait_impl,
    reason = "Trusted in-memory authority follows the async port without inserting artificial yields."
)]
impl RequestAuthority for Authority {
    type Actor = Actor;
    async fn authorize(
        &self,
        actor: &Actor,
        target: EntityId,
        _: QueueAction,
    ) -> Result<ActorContext, RegistryError> {
        self.write_calls.fetch_add(1, Ordering::SeqCst);
        if self.deny_write.load(Ordering::SeqCst) {
            return Err(RegistryError::InvalidActor);
        }
        if !self.targets.read().unwrap().contains_key(&target) {
            return Err(RegistryError::InvalidTarget);
        }
        Ok(ActorContext {
            client_id: actor.id,
            auth_generation: actor.generation,
        })
    }
    async fn authorize_read(
        &self,
        actor: &Actor,
        target: EntityId,
    ) -> Result<ActorContext, RegistryError> {
        if self.deny_read.load(Ordering::SeqCst) {
            return Err(RegistryError::InvalidActor);
        }
        if !self.targets.read().unwrap().contains_key(&target) {
            return Err(RegistryError::InvalidTarget);
        }
        Ok(ActorContext {
            client_id: actor.id,
            auth_generation: actor.generation,
        })
    }
    async fn resolve_target(&self, target: EntityId) -> Result<NativeSessionKey, RegistryError> {
        self.targets
            .read()
            .unwrap()
            .get(&target)
            .cloned()
            .ok_or(RegistryError::InvalidTarget)
    }
}

pub struct Body {
    pub bytes: Vec<u8>,
}
#[derive(Clone, Copy)]
pub struct Validator;
impl ValidatedBodyPort for Validator {
    type Raw = [u8];
    type Body = Body;
    fn validate(&self, raw: &[u8]) -> Result<(Body, [u8; 32]), RegistryError> {
        Ok((
            Body {
                bytes: raw.to_vec(),
            },
            Sha256::digest(raw).into(),
        ))
    }
}
#[derive(Clone)]
pub struct Clock(pub Arc<AtomicI64>);
impl Clock {
    pub fn new() -> Self {
        Self(Arc::new(AtomicI64::new(1_000)))
    }
}
impl NodeClock for Clock {
    fn unix_millis(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

pub struct Environment {
    state: Mutex<HashMap<NativeSessionKey, EnvironmentState>>,
    pub readiness_calls: AtomicU64,
}
struct EnvironmentState {
    ready: DispatchReadiness,
    resource: Option<ResourceEpoch>,
    events: VecDeque<QueueOwnerEvent>,
}
impl Environment {
    pub fn new() -> Self {
        let state = [1, 2]
            .into_iter()
            .map(|source| {
                (
                    native(source),
                    EnvironmentState {
                        ready: DispatchReadiness::Blocked,
                        resource: Some(resource(103)),
                        events: VecDeque::new(),
                    },
                )
            })
            .collect();
        Self {
            state: Mutex::new(state),
            readiness_calls: AtomicU64::new(0),
        }
    }
    pub fn set_readiness(&self, target: &NativeSessionKey, readiness: DispatchReadiness) {
        self.state.lock().unwrap().get_mut(target).unwrap().ready = readiness;
    }
    pub fn set_resource(&self, target: &NativeSessionKey, epoch: ResourceEpoch) {
        self.state.lock().unwrap().get_mut(target).unwrap().resource = Some(epoch);
    }
    pub fn push(&self, target: &NativeSessionKey, event: QueueOwnerEvent) {
        self.state
            .lock()
            .unwrap()
            .get_mut(target)
            .unwrap()
            .events
            .push_back(event);
    }
}
impl QueueEnvironmentPort for Environment {
    fn readiness(&self, target: &NativeSessionKey) -> DispatchReadiness {
        self.readiness_calls.fetch_add(1, Ordering::SeqCst);
        self.state
            .lock()
            .unwrap()
            .get(target)
            .map_or(DispatchReadiness::Blocked, |s| s.ready)
    }
    fn resource_epoch(&self, target: &NativeSessionKey) -> Option<ResourceEpoch> {
        self.state
            .lock()
            .unwrap()
            .get(target)
            .and_then(|s| s.resource)
    }
    fn poll_event(&self, target: &NativeSessionKey) -> Option<QueueOwnerEvent> {
        self.state
            .lock()
            .unwrap()
            .get_mut(target)
            .and_then(|s| s.events.pop_front())
    }
}

pub struct Gate {
    reached: AtomicBool,
    released: AtomicBool,
    arrived: Notify,
    resume: Notify,
}
impl Gate {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            reached: AtomicBool::new(false),
            released: AtomicBool::new(false),
            arrived: Notify::new(),
            resume: Notify::new(),
        })
    }
    pub async fn enter(&self) {
        self.reached.store(true, Ordering::SeqCst);
        self.arrived.notify_one();
        while !self.released.load(Ordering::SeqCst) {
            self.resume.notified().await;
        }
    }
    pub async fn wait(&self) {
        timeout(deadline(), async {
            while !self.reached.load(Ordering::SeqCst) {
                self.arrived.notified().await;
            }
        })
        .await
        .expect("test barrier was not reached");
    }
    pub fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        self.resume.notify_one();
    }
}

#[derive(Clone)]
pub enum Action {
    Pause(Arc<Gate>),
    ReturnUnconfirmed,
    ReturnUnavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Point {
    RegisterAfter,
    BeginAfter,
    SettleBefore,
    SettleAfter,
    StartupBefore,
    StartupAfter,
    InspectBefore,
}
#[derive(Clone, Copy)]
pub enum InspectionCorruption {
    Operation,
    Runtime,
    Generation,
    BegunAt,
    Intent,
}
#[derive(Clone)]
pub struct Control {
    actions: Arc<Mutex<HashMap<Point, VecDeque<Action>>>>,
    pub begin_calls: Arc<AtomicU64>,
    pub settle_calls: Arc<AtomicU64>,
    pub settle_values: Arc<Mutex<Vec<(RegistrationId, QueueOutcome)>>>,
    pub inspection_corruption: Arc<Mutex<Option<InspectionCorruption>>>,
}
impl Control {
    pub fn new() -> Self {
        Self {
            actions: Arc::new(Mutex::new(HashMap::new())),
            begin_calls: Arc::new(AtomicU64::new(0)),
            settle_calls: Arc::new(AtomicU64::new(0)),
            settle_values: Arc::new(Mutex::new(Vec::new())),
            inspection_corruption: Arc::new(Mutex::new(None)),
        }
    }
    pub fn arm(&self, point: Point, action: Action) {
        self.actions
            .lock()
            .unwrap()
            .entry(point)
            .or_default()
            .push_back(action);
    }
    async fn act(&self, point: Point) -> Result<(), RegistryError> {
        let action = {
            self.actions
                .lock()
                .unwrap()
                .get_mut(&point)
                .and_then(VecDeque::pop_front)
        };
        match action {
            None => Ok(()),
            Some(Action::Pause(gate)) => {
                gate.enter().await;
                Ok(())
            }
            Some(Action::ReturnUnconfirmed) => Err(RegistryError::CommitUnconfirmed),
            Some(Action::ReturnUnavailable) => Err(RegistryError::StorageUnavailable),
        }
    }
}
#[derive(Clone)]
pub struct Repository<R> {
    inner: R,
    pub control: Control,
}
impl<R> Repository<R> {
    pub const fn new(inner: R, control: Control) -> Self {
        Self { inner, control }
    }
}
impl<R: RequestRegistryRepository> RequestRegistryRepository for Repository<R> {
    async fn register(
        &self,
        actor: &CurrentRequestActor,
        intent: &RegisteredIntent,
        id: RegistrationId,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> Result<RegistrationFact, RegistryError> {
        let fact = self.inner.register(actor, intent, id, at, deadline).await?;
        self.control.act(Point::RegisterAfter).await?;
        Ok(fact)
    }
    async fn begin_consume(
        &self,
        actor: &CurrentRequestActor,
        id: RegistrationId,
        intent: &RegisteredIntent,
        operation: EntityId,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> Result<BeginRecord, RegistryError> {
        self.control.begin_calls.fetch_add(1, Ordering::SeqCst);
        let fact = self
            .inner
            .begin_consume(actor, id, intent, operation, at, deadline)
            .await?;
        self.control.act(Point::BeginAfter).await?;
        Ok(fact)
    }
    async fn settle(
        &self,
        lease: &ConsumeLease,
        outcome: &QueueOutcome,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        self.control.settle_calls.fetch_add(1, Ordering::SeqCst);
        self.control
            .settle_values
            .lock()
            .unwrap()
            .push((lease.registration_id(), *outcome));
        self.control.act(Point::SettleBefore).await?;
        let fact = self.inner.settle(lease, outcome, at, deadline).await?;
        self.control.act(Point::SettleAfter).await?;
        Ok(fact)
    }
    async fn read(
        &self,
        reader: &CurrentRequestActor,
        target: EntityId,
        id: RegistrationId,
        deadline: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        self.inner.read(reader, target, id, deadline).await
    }
    async fn inspect_operation(
        &self,
        probe: &OperationProbe,
        deadline: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        self.control.act(Point::InspectBefore).await?;
        let mut fact = self.inner.inspect_operation(probe, deadline).await?;
        let corruption = self.control.inspection_corruption.lock().unwrap().take();
        if let (Some(kind), RequestFact::Applying(stored)) = (corruption, &mut fact) {
            // Explicit negative trusted-port fault injection, after a genuine SQL read.
            // This branch is not counted as a normal SQLite metadata guarantee.
            match kind {
                InspectionCorruption::Operation => stored.operation = id(241),
                InspectionCorruption::Runtime => stored.runtime = RuntimeEpoch(id(242)),
                InspectionCorruption::Generation => stored.admission_auth_generation += 1,
                InspectionCorruption::BegunAt => stored.begun_at_ms += 1,
                InspectionCorruption::Intent => {
                    let old = &stored.registration;
                    let intent = old.intent();
                    let replacement = RegisteredIntent::new(
                        intent.actor(),
                        intent.action(),
                        T2,
                        intent.item(),
                        intent.expected_revision(),
                        intent.body_digest().copied(),
                    )
                    .unwrap();
                    stored.registration = RegistrationFact::new(
                        old.id(),
                        replacement,
                        old.register_auth_generation(),
                        old.registered_at_ms(),
                    )
                    .unwrap();
                }
            }
        }
        Ok(fact)
    }
    async fn interrupt_previous_runtime(
        &self,
        current: RuntimeEpoch,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> Result<u64, RegistryError> {
        self.control.act(Point::StartupBefore).await?;
        let count = self
            .inner
            .interrupt_previous_runtime(current, at, deadline)
            .await?;
        self.control.act(Point::StartupAfter).await?;
        Ok(count)
    }
}

pub enum AuditMode {
    Pass,
    Fail(AuditFailure),
    Pause(Arc<Gate>),
}
pub struct Audit {
    pub mode: Mutex<AuditMode>,
    pub calls: AtomicU64,
    pub records: Mutex<Vec<AuditMetadata>>,
}
impl Audit {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            mode: Mutex::new(AuditMode::Pass),
            calls: AtomicU64::new(0),
            records: Mutex::new(Vec::new()),
        })
    }
}
impl AuditPort for Audit {
    fn append<'a>(
        &'a self,
        metadata: &'a AuditMetadata,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuditFailure>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.records.lock().unwrap().push(metadata.clone());
            let (gate, error) = {
                match &*self.mode.lock().unwrap() {
                    AuditMode::Pass => (None, None),
                    AuditMode::Fail(e) => (None, Some(*e)),
                    AuditMode::Pause(g) => (Some(Arc::clone(g)), None),
                }
            };
            if let Some(gate) = gate {
                gate.enter().await;
            }
            error.map_or(Ok(()), Err)
        })
    }
}

fn root() -> PathBuf {
    std::env::var_os("Q3_REGISTRATION_FIXTURE_ROOT").map_or_else(
        || std::env::temp_dir().join("q3-registration-public-fixtures"),
        PathBuf::from,
    )
}
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Area(pub PathBuf);
impl Area {
    pub fn new() -> Self {
        let root = root();
        std::fs::create_dir_all(&root).unwrap();
        let child = root.join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&child).unwrap();
        let owned = child.canonicalize().unwrap();
        assert_eq!(owned.parent(), Some(root.canonicalize().unwrap().as_path()));
        Self(owned)
    }
    pub fn db(&self) -> PathBuf {
        self.0.join("node.sqlite3")
    }
}
impl Drop for Area {
    fn drop(&mut self) {
        let root = root().canonicalize().unwrap();
        let child = self.0.canonicalize().unwrap();
        assert_eq!(child.parent(), Some(root.as_path()));
        assert_eq!(child, root.join(self.0.file_name().unwrap()));
        assert!(child.starts_with(root));
        std::fs::remove_dir_all(child).unwrap();
    }
}
pub async fn connection(path: &Path) -> SqliteConnection {
    SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true),
    )
    .await
    .unwrap()
}

pub type Service = QueueService<Authority, Validator, Repository<SqliteRequestRegistry>, Clock>;
pub struct Fixture {
    pub area: Area,
    pub storage: Storage,
    pub authority: Authority,
    pub clock: Clock,
    pub environment: Arc<Environment>,
    pub audit: Arc<Audit>,
    pub control: Control,
}
impl Fixture {
    pub async fn new() -> Self {
        let area = Area::new();
        let storage = Storage::open(&area.db()).await.unwrap();
        storage
            .write_node_meta(&NodeMeta {
                schema_version: SCHEMA_VERSION,
                node_id: "registration-synthetic-node".into(),
                public_key: vec![4, 1, 2, 0, 255],
                key_file_ref: "synthetic-unused.key".into(),
            })
            .await
            .unwrap();
        let mut sql = connection(&area.db()).await;
        for actor in [A, B, REVOKED] {
            sqlx::query("INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at,revoked_at) VALUES(?,?,?, ?,?,0,NULL)")
                .bind(actor.id.encode()).bind(actor.id.encode().into_bytes()).bind("synthetic-client")
                .bind(if actor.id == REVOKED.id { "revoked" } else { "trusted" }).bind(actor.generation).execute(&mut sql).await.unwrap();
        }
        for (source, target) in [(1, T1), (2, T2)] {
            sqlx::query("INSERT INTO product_sources(source_id,name) VALUES(?,?)")
                .bind(id(source).encode())
                .bind("synthetic-source")
                .execute(&mut sql)
                .await
                .unwrap();
            sqlx::query("INSERT INTO product_targets(target_id,source_id,native_id,workspace_id) VALUES(?,?,?,NULL)").bind(target.encode()).bind(id(source).encode()).bind("reg-q3-synthetic").execute(&mut sql).await.unwrap();
        }
        sql.close().await.unwrap();
        Self {
            area,
            storage,
            authority: Authority::new(),
            clock: Clock::new(),
            environment: Arc::new(Environment::new()),
            audit: Audit::new(),
            control: Control::new(),
        }
    }
    pub async fn start(&self) -> (Service, NativeQueueDriver<Body>) {
        let control = self.control.clone();
        self.storage
            .start_queue_service_with_repository_and_audit_for_fixture(
                self.authority.clone(),
                Validator,
                self.clock.clone(),
                self.environment.clone(),
                self.audit.clone(),
                move |inner| Ok(Repository::new(inner, control)),
            )
            .await
            .unwrap()
    }
    pub async fn close(&self, service: &Service) {
        let report = service.shutdown(closing_deadline()).await;
        assert!(report.joined);
        assert!(report.unresolved.is_empty());
        self.storage.close().await;
    }
    pub async fn sql_count(&self, table: &str) -> i64 {
        let mut sql = connection(&self.area.db()).await;
        let n = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&mut sql)
            .await
            .unwrap();
        sql.close().await.unwrap();
        n
    }
    pub async fn phase(&self, request: RegistrationId) -> Option<String> {
        let mut sql = connection(&self.area.db()).await;
        let phase =
            sqlx::query_scalar("SELECT phase FROM node_request_operations WHERE request_id=?")
                .bind(request.encode())
                .fetch_optional(&mut sql)
                .await
                .unwrap();
        sql.close().await.unwrap();
        phase
    }
    pub async fn revoke(&self, actor: Actor) {
        let mut sql = connection(&self.area.db()).await;
        sqlx::query("UPDATE clients SET status='revoked',auth_generation=auth_generation+1 WHERE client_id=?").bind(actor.id.encode()).execute(&mut sql).await.unwrap();
        sql.close().await.unwrap();
    }
}

pub fn submit(service: &Service, bytes: &[u8]) -> ValidatedMutation<Body> {
    ValidatedMutation::Submit(service.validate_body(bytes).unwrap())
}
pub async fn registered(
    service: &Service,
    actor: &Actor,
    target: EntityId,
    bytes: &[u8],
) -> RegistrationFact {
    let mutation = submit(service, bytes);
    service.register(actor, target, &mutation).await.unwrap()
}
pub fn resolved(
    reply: acp_remote::application::queue_owner::ConsumeReply<Body>,
) -> (RequestFact, Option<ValidatedMutation<Body>>) {
    match reply {
        acp_remote::application::queue_owner::ConsumeReply::Resolved { fact, returned } => {
            (fact, returned)
        }
        acp_remote::application::queue_owner::ConsumeReply::Pending { .. } => {
            panic!("expected reliable resolved management fact")
        }
        acp_remote::application::queue_owner::ConsumeReply::Refused { error, .. } => {
            panic!("unexpected management refusal: {error:?}")
        }
    }
}
pub fn outcome(fact: &RequestFact) -> QueueOutcome {
    match fact {
        RequestFact::Settled { outcome, .. } => *outcome,
        _ => panic!("expected stable management outcome"),
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: EntityId,
    pub revision: u64,
    pub submitter: EntityId,
    pub editor: EntityId,
    pub suspended: bool,
    pub bytes: Vec<u8>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct View {
    pub processing: bool,
    pub applied: bool,
    pub waiting: Vec<Entry>,
    pub active: Option<Entry>,
    pub failed: bool,
}
fn entry(
    e: &acp_remote::domain::input_queue::QueueEntry<
        acp_remote::application::request_registry::ValidatedBody<Body>,
    >,
) -> Entry {
    Entry {
        id: e.id(),
        revision: e.revision(),
        submitter: e.input().submitter,
        editor: e.editor(),
        suspended: e.suspended(),
        bytes: e.input().body.body().bytes.clone(),
    }
}
pub async fn view(service: &Service, actor: &Actor, target: EntityId) -> View {
    service
        .inspect_queue(actor, target, |read| {
            let (queue, processing, applied) = match read {
                QueueRead::Stable { queue } => (queue, false, false),
                QueueRead::Processing { queue, applied, .. } => (queue, true, applied),
            };
            View {
                processing,
                applied,
                waiting: queue.waiting().map(entry).collect(),
                active: queue.active().map(entry),
                failed: queue.failure_suspended(),
            }
        })
        .await
        .unwrap()
}
