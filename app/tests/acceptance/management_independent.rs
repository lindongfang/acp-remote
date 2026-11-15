//! Independent management-stage tests, disposable SQLite and synthetic trusted ports only.
#[path = "management_deadline_semantics.rs"]
mod deadline_semantics;
use super::{Fault, ProductStore};
use crate::{
    adapters::product_requests::{self as jws_request, ProductRequestSigner},
    application::product_commands::{
        Cancellation, InputAdmission, InputAuthority, InputOwner, InputStructureValidator,
        InputTransport, MemoryInputs, NativeInputReport, ProductInputService, RequestIdentityPort,
        TemporaryInput, TimeSource,
    },
    domain::{
        ActorContext, EntityId,
        activity::WorkActivity,
        native::{
            HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, RequestEpoch, ResourceEpoch,
            RuntimeEpoch,
        },
        product_commands::{
            AUDIT_RETENTION_MS, InputReceipt, InputResult, InputTarget,
            ProductCommandError as Error, ProductCommandFact, ProductCommandStatus, audit_room,
            input_intent,
        },
        requests::{
            RequestClock, RequestIdentityError, RequestIntent, RequestTime, VerifiedProductRequest,
        },
    },
    jws::{self, JwsError, JwsType},
    storage::Storage,
};
use p256::ecdsa::SigningKey;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex as AsyncMutex, Notify},
    time::{Instant, timeout},
};

const NOW: u64 = 1_700_000_000_000;
const BODY: &[u8] = br#"[{"type":"synthetic","text":"INDEPENDENT_BODY_MARKER"}]"#;
fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
#[derive(Clone)]
struct Clock {
    inner: Arc<Mutex<RequestClock>>,
    now: Arc<AtomicU64>,
    calls: Arc<AtomicU64>,
}
impl Clock {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RequestClock::new(0, NOW).unwrap())),
            now: Arc::new(AtomicU64::new(NOW)),
            calls: Arc::new(AtomicU64::new(0)),
        }
    }
    fn set(&self, now: u64) {
        self.now.store(now, Ordering::SeqCst);
    }
}
impl TimeSource for Clock {
    fn sample(&self) -> Result<RequestTime, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let now = self.now.load(Ordering::SeqCst);
        self.inner
            .lock()
            .unwrap()
            .observe(
                Some(now),
                Some(Duration::from_millis(now.saturating_sub(NOW))),
            )
            .map_err(|_| Error::ClockUnreliable)
    }
}
struct Codec {
    key: SigningKey,
    node: String,
}
impl Codec {
    fn new() -> Self {
        let key = SigningKey::from_slice(&[11; 32]).unwrap();
        let node = hex::encode(Sha256::digest(
            key.verifying_key().to_sec1_point(false).as_bytes(),
        ));
        Self { key, node }
    }
    fn public(&self) -> Vec<u8> {
        self.key
            .verifying_key()
            .to_sec1_point(false)
            .as_bytes()
            .to_vec()
    }
}
impl ProductRequestSigner for Codec {
    fn node_id(&self) -> &str {
        &self.node
    }
    fn sign_product_request(&self, body: &[u8]) -> Result<String, JwsError> {
        jws::sign(JwsType::ProductRequest, body, &self.key)
    }
}
fn identity_error(error: RequestIdentityError) -> Error {
    match error {
        RequestIdentityError::Expired => Error::Expired,
        RequestIdentityError::EpochMismatch => Error::EpochMismatch,
        RequestIdentityError::ClockUnreliable => Error::ClockUnreliable,
        _ => Error::InvalidIdentity,
    }
}
impl RequestIdentityPort for Codec {
    fn issue(
        &self,
        epoch: RequestEpoch,
        request: EntityId,
        intent: &RequestIntent,
        time: RequestTime,
    ) -> Result<String, Error> {
        jws_request::issue(self, epoch, request, intent, time)
            .map(|s| s.compact().to_owned())
            .map_err(identity_error)
    }
    fn verify(
        &self,
        token: &str,
        epoch: RequestEpoch,
        intent: &RequestIntent,
        time: RequestTime,
    ) -> Result<VerifiedProductRequest, Error> {
        jws_request::verify(token, &self.public(), epoch, intent, time).map_err(identity_error)
    }
}
#[derive(Clone)]
struct Actor(ActorContext);
#[derive(Clone)]
struct Authority {
    pool: SqlitePool,
    gate: Arc<AsyncMutex<()>>,
    owners: Arc<Mutex<HashMap<EntityId, Arc<AsyncMutex<InputOwner>>>>>,
    pause: Arc<AtomicBool>,
    arrived: Arc<Notify>,
    release: Arc<Notify>,
}
impl InputAuthority for Authority {
    type Actor = Actor;
    fn actor_gate(&self, _actor: &Actor) -> Result<Arc<AsyncMutex<()>>, Error> {
        Ok(self.gate.clone())
    }
    async fn require_current(
        &self,
        actor: &Actor,
        target: EntityId,
    ) -> Result<ActorContext, Error> {
        if !self.owners.lock().unwrap().contains_key(&target) {
            return Err(Error::InvalidTarget);
        }
        let valid:i64=sqlx::query_scalar("SELECT count(*) FROM clients WHERE client_id=? AND auth_generation=? AND status='trusted'").bind(actor.0.client_id.encode()).bind(actor.0.auth_generation).fetch_one(&self.pool).await?;
        if valid != 1 {
            return Err(Error::InvalidActor);
        }
        Ok(actor.0)
    }
    #[allow(
        clippy::unused_async_trait_impl,
        reason = "Resolve ownership when the trusted future is polled, preserving cancellation ordering"
    )]
    async fn owner(&self, target: EntityId) -> Result<Arc<AsyncMutex<InputOwner>>, Error> {
        self.owners
            .lock()
            .unwrap()
            .get(&target)
            .cloned()
            .ok_or(Error::InvalidTarget)
    }
    async fn refresh_owner(&self, _owner: &mut InputOwner) -> Result<(), Error> {
        if self.pause.swap(false, Ordering::SeqCst) {
            self.arrived.notify_one();
            self.release.notified().await;
        }
        Ok(())
    }
}
struct Structure;
impl InputStructureValidator for Structure {
    fn validate(&self, body: &[u8]) -> Result<(), Error> {
        if serde_json::from_slice::<Value>(body).is_ok_and(|v| v.is_array()) {
            Ok(())
        } else {
            Err(Error::InvalidInput)
        }
    }
}
type Service = ProductInputService<Authority, Codec, Clock, ProductStore>;
struct Fixture {
    storage: Storage,
    pool: SqlitePool,
    store: ProductStore,
    clock: Clock,
    authority: Authority,
    actor: Actor,
    service: Arc<Service>,
    path: PathBuf,
}
impl Fixture {
    async fn new() -> Self {
        let root = std::env::temp_dir().join("acpr-management-acceptance");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "{}.sqlite3",
            hex::encode(crate::random::bytes::<16>().unwrap())
        ));
        let storage = Storage::open(&path).await.unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .foreign_keys(true)
                    .busy_timeout(Duration::from_secs(5)),
            )
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&pool)
                .await
                .unwrap(),
            crate::storage::SCHEMA_VERSION
        );
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='product_delivery_attempts'").fetch_one(&pool).await.unwrap(), 1);
        sqlx::query("INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at) VALUES(?,X'01','synthetic','trusted',3,0)").bind(id(1).encode()).execute(&pool).await.unwrap();
        let store = storage.product_commands();
        let clock = Clock::new();
        let authority = Authority {
            pool: pool.clone(),
            gate: Arc::default(),
            owners: Arc::default(),
            pause: Arc::new(AtomicBool::new(false)),
            arrived: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        };
        let service = Arc::new(
            ProductInputService::start(
                store.clone(),
                authority.clone(),
                Codec::new(),
                clock.clone(),
            )
            .await
            .unwrap(),
        );
        Self {
            storage,
            pool,
            store,
            clock,
            authority,
            actor: Actor(ActorContext {
                client_id: id(1),
                auth_generation: 3,
            }),
            service,
            path,
        }
    }
    async fn target(&self, n: u8) -> InputTarget {
        let target = InputTarget {
            target: id(n),
            native: NativeSessionKey::from_config(Some(id(2)), &format!("native-{n}")).unwrap(),
            epoch: ResourceEpoch {
                runtime: RuntimeEpoch(id(3)),
                connection: HarnessConnectionEpoch(id(4)),
                load: LoadEpoch(id(n.wrapping_add(60))),
            },
            instance: id(5),
            revision: 0,
        };
        sqlx::query("INSERT OR IGNORE INTO product_sources VALUES(?,'synthetic')")
            .bind(id(2).encode())
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
            .bind(target.target.encode())
            .bind(id(2).encode())
            .bind(target.native.native_id())
            .execute(&self.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_resources VALUES(?,?,?,?,?,'loaded',0)")
            .bind(target.epoch.load.0.encode())
            .bind(target.target.encode())
            .bind(target.epoch.runtime.0.encode())
            .bind(target.epoch.connection.0.encode())
            .bind(target.instance.encode())
            .execute(&self.pool)
            .await
            .unwrap();
        let mut owner = InputOwner::from_verified_source(target.clone(), true).unwrap();
        owner.observe_foreground(WorkActivity::Idle).unwrap();
        self.authority
            .owners
            .lock()
            .unwrap()
            .insert(target.target, Arc::new(AsyncMutex::new(owner)));
        target
    }
    async fn token(&self, target: &InputTarget) -> String {
        self.service
            .issue(
                &self.actor,
                target.target,
                Sha256::digest(BODY).into(),
                &Cancellation::default(),
            )
            .await
            .unwrap()
    }
    async fn body(&self, target: &InputTarget, inputs: &MemoryInputs) -> TemporaryInput {
        let mut upload = self
            .service
            .prepare_input(
                &self.actor,
                target.target,
                inputs,
                BODY.len(),
                0,
                &Cancellation::default(),
            )
            .await
            .unwrap();
        upload.append(BODY).unwrap();
        upload.finish(&Structure).unwrap()
    }
    async fn admitted(
        &self,
        target: &InputTarget,
        token: &str,
        inputs: &MemoryInputs,
    ) -> (
        crate::domain::product_commands::ProductCommandFact,
        crate::application::product_commands::InputTicket,
    ) {
        match self
            .service
            .submit(
                &self.actor,
                target.target,
                token,
                self.body(target, inputs).await,
                &Cancellation::default(),
            )
            .await
            .unwrap()
        {
            InputAdmission::New { fact, ticket } => (fact, ticket),
            InputAdmission::Replay(_) => panic!("new ticket required"),
        }
    }
    async fn close(self) {
        self.pool.close().await;
        self.storage.close().await;
    }
    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
    async fn proof(&self, target: &InputTarget, request: u8) -> VerifiedProductRequest {
        let codec = Codec::new();
        let intent =
            input_intent(self.actor.0.client_id, target, Sha256::digest(BODY).into()).unwrap();
        let epoch = self.store.control().await.unwrap().0;
        let time = self.clock.sample().unwrap();
        let token = codec.issue(epoch, id(request), &intent, time).unwrap();
        codec.verify(&token, epoch, &intent, time).unwrap()
    }
    async fn admit(
        &self,
        target: &InputTarget,
        proof: &VerifiedProductRequest,
    ) -> Result<(ProductCommandFact, bool), Error> {
        self.store
            .admit(
                self.actor.0,
                proof,
                target,
                true,
                &self.clock,
                proof.identity().epoch,
                deadline(),
            )
            .await
    }
    async fn second_store(&self) -> (ProductStore, SqlitePool) {
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&self.path)
                    .foreign_keys(true)
                    .busy_timeout(Duration::from_secs(5)),
            )
            .await
            .unwrap();
        (ProductStore::new(pool.clone()), pool)
    }
    async fn private_claim(
        &self,
        target: &InputTarget,
        proof: &VerifiedProductRequest,
        fact: &ProductCommandFact,
    ) -> InputReceipt {
        let claim = EntityId::from_bytes(crate::random::bytes().unwrap());
        assert!(
            self.store
                .claim(
                    self.actor.0,
                    proof,
                    target,
                    fact,
                    claim,
                    &self.clock,
                    proof.identity().epoch,
                    deadline()
                )
                .await
                .unwrap()
        );
        InputReceipt {
            command: fact.command,
            run: fact.run,
            claim,
            target: target.clone(),
            owner_nonce: id(88),
        }
    }
}
#[derive(Default)]
struct Writer {
    calls: AtomicU64,
}
impl InputTransport for Writer {
    #[allow(
        clippy::unused_async_trait_impl,
        reason = "Count a write only when its future is polled, after the final cancellation fence"
    )]
    async fn write_once(&self, body: &[u8]) -> Result<(), Error> {
        assert_eq!(body, BODY);
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn m_input_budget_request_expiry_caps_retained_ticket_body() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let token = f.token(&target).await;
    f.clock.set(NOW + 86_400_000 - 500);
    let inputs = MemoryInputs::default();
    let supervisor = inputs.supervise();
    let (_fact, ticket) = f.admitted(&target, &token, &inputs).await;
    assert_eq!(inputs.retained(), (BODY.len(), 1));
    f.clock.set(NOW + 86_400_000 + 1);
    tokio::time::sleep(Duration::from_millis(700)).await;
    let after_expiry = inputs.retained();
    drop(ticket);
    supervisor.shutdown().await;
    f.close().await;
    assert_eq!(
        after_expiry,
        (0, 0),
        "holding an accepted ticket must not extend input beyond signed expiry"
    );
}

#[tokio::test]
async fn m_final_fence_shared_fault_after_commit_prevents_first_write() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let token = f.token(&target).await;
    let inputs = MemoryInputs::default();
    let (_fact, ticket) = f.admitted(&target, &token, &inputs).await;
    let permit = f
        .service
        .claim(&f.actor, ticket, &Cancellation::default())
        .await
        .unwrap();
    f.authority.pause.store(true, Ordering::SeqCst);
    let writer = Arc::new(Writer::default());
    let task_writer = writer.clone();
    let service = f.service.clone();
    let actor = f.actor.clone();
    let task = tokio::spawn(async move {
        service
            .dispatch(
                &actor,
                permit,
                task_writer.as_ref(),
                &Cancellation::default(),
            )
            .await
    });
    timeout(Duration::from_secs(2), f.authority.arrived.notified())
        .await
        .unwrap();
    assert!(f.pool.acquire().await.is_ok());
    f.store.fault();
    assert!(f.storage.product_commands().faulted());
    f.authority.release.notify_one();
    let result = task.await.unwrap();
    let calls = writer.calls.load(Ordering::SeqCst);
    f.close().await;
    assert_eq!(
        calls, 0,
        "fault observed after final fence commit still forbids a first write"
    );
    assert!(
        result.is_err(),
        "shared management fault must close dispatch qualification"
    );
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep ordered lifecycle and immutable-slot assertions together at their actual boundaries"
)]
async fn m_public_lifecycle_replay_insertion_completion_and_immutable_slots() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let token = f.token(&target).await;
    let inputs = MemoryInputs::default();
    let (fact, ticket) = f.admitted(&target, &token, &inputs).await;
    assert_eq!(
        (
            f.count("product_commands").await,
            f.count("product_runs").await,
            f.count("product_audit_slots").await
        ),
        (1, 1, 4)
    );
    let replay = f
        .service
        .submit(
            &f.actor,
            target.target,
            &token,
            f.body(&target, &inputs).await,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    assert!(matches!(replay, InputAdmission::Replay(_)));
    assert_eq!(replay.fact().command, fact.command);
    let permit = f
        .service
        .claim(&f.actor, ticket, &Cancellation::default())
        .await
        .unwrap();
    let writer = Writer::default();
    let receipt = f
        .service
        .dispatch(&f.actor, permit, &writer, &Cancellation::default())
        .await
        .unwrap();
    assert_eq!(writer.calls.load(Ordering::SeqCst), 1);
    assert_eq!(inputs.retained(), (0, 0));
    f.service
        .report(
            &receipt,
            NativeInputReport::TransportWritten,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        f.service
            .command(
                &f.actor,
                target.target,
                fact.command,
                &Cancellation::default()
            )
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Dispatching
    );
    f.service
        .report(
            &receipt,
            NativeInputReport::Inserted,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM product_runs")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "running"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='result' AND occurred_at IS NOT NULL").fetch_one(&f.pool).await.unwrap(),0);
    f.service
        .report(
            &receipt,
            NativeInputReport::Completed,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    let first = sqlx::query(
        "SELECT occurred_at,retain_until,outcome FROM product_audit_slots WHERE stage='result'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        first.get::<i64, _>("retain_until") - first.get::<i64, _>("occurred_at"),
        i64::try_from(AUDIT_RETENTION_MS).unwrap()
    );
    f.clock.set(NOW + 100);
    f.service
        .report(
            &receipt,
            NativeInputReport::Completed,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    let second = sqlx::query(
        "SELECT occurred_at,retain_until FROM product_audit_slots WHERE stage='result'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        first.get::<i64, _>("occurred_at"),
        second.get::<i64, _>("occurred_at")
    );
    assert_eq!(
        first.get::<i64, _>("retain_until"),
        second.get::<i64, _>("retain_until")
    );
    assert_eq!(
        f.service
            .report(
                &receipt,
                NativeInputReport::Unknown,
                &Cancellation::default()
            )
            .await
            .err(),
        Some(Error::Conflict)
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='reconciliation' AND (occurred_at IS NOT NULL OR retain_until IS NOT NULL OR outcome IS NOT NULL)").fetch_one(&f.pool).await.unwrap(),0);
    assert_eq!(writer.calls.load(Ordering::SeqCst), 1);
    f.close().await;
}

#[tokio::test]
async fn m_two_pools_same_request_single_command_and_single_claim() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let proof = f.proof(&target, 31).await;
    let (other, pool) = f.second_store().await;
    let epoch = proof.identity().epoch;
    let (a, b) = tokio::join!(
        f.admit(&target, &proof),
        other.admit(
            f.actor.0,
            &proof,
            &target,
            true,
            &f.clock,
            epoch,
            deadline()
        )
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.0.command, b.0.command);
    assert_ne!(a.1, b.1);
    assert_eq!(
        (
            f.count("product_commands").await,
            f.count("product_runs").await,
            f.count("product_audit_slots").await
        ),
        (1, 1, 4)
    );
    let (a, b) = tokio::join!(
        f.store.claim(
            f.actor.0,
            &proof,
            &target,
            &a.0,
            id(91),
            &f.clock,
            epoch,
            deadline()
        ),
        other.claim(
            f.actor.0,
            &proof,
            &target,
            &b.0,
            id(92),
            &f.clock,
            epoch,
            deadline()
        )
    );
    assert_ne!(a.unwrap(), b.unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM product_commands WHERE claim_id IS NOT NULL"
        )
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    assert!(
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
            .bind(id(99).encode())
            .bind(id(2).encode())
            .bind(target.native.native_id())
            .execute(&f.pool)
            .await
            .is_err()
    );
    pool.close().await;
    f.close().await;
}

#[tokio::test]
async fn m_two_pools_same_target_busy_but_distinct_targets_parallel() {
    let fixture = Fixture::new().await;
    let target = fixture.target(10).await;
    let second = fixture.target(11).await;
    let first_proof = fixture.proof(&target, 31).await;
    let second_proof = fixture.proof(&target, 32).await;
    let (store, pool) = fixture.second_store().await;
    let epoch = first_proof.identity().epoch;
    let (first_result, second_result) = tokio::join!(
        fixture.admit(&target, &first_proof),
        store.admit(
            fixture.actor.0,
            &second_proof,
            &target,
            true,
            &fixture.clock,
            epoch,
            deadline()
        )
    );
    assert_ne!(first_result.is_ok(), second_result.is_ok());
    assert_eq!(
        first_result.err().or_else(|| second_result.err()),
        Some(Error::Busy)
    );
    assert!(
        fixture
            .admit(&second, &fixture.proof(&second, 33).await)
            .await
            .is_ok()
    );
    assert_eq!(fixture.count("product_commands").await, 2);
    pool.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn m_required_admission_commit_failures_and_unknown_never_return_confirmation() {
    let cases = (1..=9).map(Fault::Step).chain([
        Fault::DeferredCommit,
        Fault::CommitUnknown,
        Fault::CasZero,
    ]);
    for fault in cases {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        let proof = f.proof(&target, 31).await;
        f.clock.set(NOW + 1);
        *f.store.fault.lock().unwrap() = Some(fault);
        assert!(f.admit(&target, &proof).await.is_err(), "fault {fault:?}");
        assert!(f.store.faulted());
        assert!(f.storage.product_commands().faulted());
        let durable = matches!(fault, Fault::CommitUnknown);
        assert_eq!(
            f.count("product_commands").await,
            i64::from(durable),
            "fault {fault:?}"
        );
        assert_eq!(f.count("product_runs").await, i64::from(durable));
        assert_eq!(
            f.count("product_audit_slots").await,
            if durable { 4 } else { 0 }
        );
        assert!(f.admit(&target, &proof).await.is_err());
        f.close().await;
    }
}

#[tokio::test]
async fn m_required_claim_commit_faults_do_not_return_claim_or_reset_to_accepted() {
    for fault in (1..=5)
        .map(Fault::Step)
        .chain([Fault::DeferredCommit, Fault::CommitUnknown])
    {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        let proof = f.proof(&target, 31).await;
        let (fact, _) = f.admit(&target, &proof).await.unwrap();
        *f.store.fault.lock().unwrap() = Some(fault);
        assert!(
            f.store
                .claim(
                    f.actor.0,
                    &proof,
                    &target,
                    &fact,
                    id(90),
                    &f.clock,
                    proof.identity().epoch,
                    deadline()
                )
                .await
                .is_err()
        );
        let status: String = sqlx::query_scalar("SELECT status FROM product_commands")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(
            status,
            if matches!(fault, Fault::CommitUnknown) {
                "dispatching"
            } else {
                "accepted"
            }
        );
        assert!(f.store.faulted());
        assert!(
            f.store
                .claim(
                    f.actor.0,
                    &proof,
                    &target,
                    &fact,
                    id(91),
                    &f.clock,
                    proof.identity().epoch,
                    deadline()
                )
                .await
                .is_err()
        );
        f.close().await;
    }
}

#[tokio::test]
async fn m_result_commit_fault_grid_preserves_original_possible_send() {
    for outcome in [
        InputResult::Completed,
        InputResult::Rejected,
        InputResult::Unknown,
    ] {
        for fault in (1..=4)
            .map(Fault::Step)
            .chain([Fault::DeferredCommit, Fault::CommitUnknown])
        {
            let f = Fixture::new().await;
            let target = f.target(10).await;
            let proof = f.proof(&target, 31).await;
            let (fact, _) = f.admit(&target, &proof).await.unwrap();
            let receipt = f.private_claim(&target, &proof, &fact).await;
            *f.store.fault.lock().unwrap() = Some(fault);
            assert!(
                f.store
                    .report(&receipt, outcome, &f.clock, deadline())
                    .await
                    .is_err()
            );
            assert!(f.store.faulted());
            let before: String = sqlx::query_scalar("SELECT status FROM product_commands")
                .fetch_one(&f.pool)
                .await
                .unwrap();
            assert_eq!(
                before,
                if matches!(fault, Fault::CommitUnknown) {
                    outcome.labels().0
                } else {
                    "dispatching"
                }
            );
            f.store
                .report(&receipt, outcome, &f.clock, deadline())
                .await
                .unwrap();
            let saved: String = sqlx::query_scalar("SELECT status FROM product_commands")
                .fetch_one(&f.pool)
                .await
                .unwrap();
            assert_eq!(saved, outcome.labels().0);
            assert!(
                f.store
                    .claim(
                        f.actor.0,
                        &proof,
                        &target,
                        &fact,
                        id(92),
                        &f.clock,
                        proof.identity().epoch,
                        deadline()
                    )
                    .await
                    .is_err()
            );
            f.close().await;
        }
    }
}

#[tokio::test]
async fn m_checkpoint_samples_after_real_begin_and_cas_uses_current_db_floor() {
    let f = Fixture::new().await;
    let epoch = f.store.control().await.unwrap().0;
    let mut locked = f.pool.acquire().await.unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *locked)
        .await
        .unwrap();
    let before = f.clock.calls.load(Ordering::SeqCst);
    let store = f.store.clone();
    let clock = f.clock.clone();
    let actor = f.actor.0;
    let task =
        tokio::spawn(async move { store.checkpoint(actor, epoch, &clock, deadline()).await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(f.clock.calls.load(Ordering::SeqCst), before);
    f.clock.set(NOW + 20);
    sqlx::query("UPDATE product_control SET time_floor_ms=?")
        .bind(i64::try_from(NOW + 10).unwrap())
        .execute(&mut *locked)
        .await
        .unwrap();
    sqlx::query("COMMIT").execute(&mut *locked).await.unwrap();
    drop(locked);
    assert_eq!(task.await.unwrap().unwrap().millis(), NOW + 20);
    assert_eq!(f.store.control().await.unwrap().1, NOW + 20);
    assert_eq!(
        f.store
            .checkpoint(f.actor.0, epoch, &Clock::new(), deadline())
            .await
            .err(),
        Some(Error::ClockUnreliable)
    );
    assert!(f.store.faulted());
    f.close().await;
}

#[tokio::test]
async fn m_expiry_precedes_dedupe_and_missing_record_cannot_revive() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let proof = f.proof(&target, 31).await;
    let missing = f.proof(&target, 32).await;
    let (fact, _) = f.admit(&target, &proof).await.unwrap();
    let end = proof.identity().expires_at;
    f.clock.set(end - 1);
    assert_eq!(
        f.admit(&target, &proof).await.unwrap().0.command,
        fact.command
    );
    f.clock.set(end);
    assert_eq!(f.admit(&target, &proof).await.err(), Some(Error::Expired));
    assert_eq!(f.admit(&target, &missing).await.err(), Some(Error::Expired));
    assert_eq!(f.count("product_commands").await, 1);
    assert_eq!(f.store.control().await.unwrap().1, end);
    assert_eq!(
        f.store
            .claim(
                f.actor.0,
                &proof,
                &target,
                &fact,
                id(99),
                &f.clock,
                proof.identity().epoch,
                deadline()
            )
            .await
            .err(),
        Some(Error::Expired)
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM product_commands")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "expired"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='dispatch' AND occurred_at IS NOT NULL").fetch_one(&f.pool).await.unwrap(),0);
    f.close().await;
}

#[tokio::test]
async fn m_actor_target_and_no_run_native_activity_are_independent_guards() {
    for column in [
        "revision",
        "runtime_epoch",
        "connection_epoch",
        "instance_id",
        "state",
    ] {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        let proof = f.proof(&target, 31).await;
        let changed = if column == "revision" {
            "1".to_owned()
        } else if column == "state" {
            "'unknown'".to_owned()
        } else {
            format!("'{}'", id(99).encode())
        };
        sqlx::query(&format!("UPDATE product_resources SET {column}={changed}"))
            .execute(&f.pool)
            .await
            .unwrap();
        assert_eq!(
            f.admit(&target, &proof).await.err(),
            Some(Error::InvalidTarget)
        );
        assert_eq!(f.count("product_commands").await, 0);
        f.close().await;
    }
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let proof = f.proof(&target, 31).await;
    for generation in [-1, 4, 9_007_199_254_740_992] {
        assert_eq!(
            f.store
                .admit(
                    ActorContext {
                        client_id: id(1),
                        auth_generation: generation
                    },
                    &proof,
                    &target,
                    true,
                    &f.clock,
                    proof.identity().epoch,
                    deadline()
                )
                .await
                .err(),
            Some(Error::InvalidActor)
        );
    }
    let token = f.token(&target).await;
    let inputs = MemoryInputs::default();
    f.authority
        .owner(target.target)
        .await
        .unwrap()
        .lock()
        .await
        .observe_foreground(WorkActivity::Active)
        .unwrap();
    let payload = f.body(&target, &inputs).await;
    assert_eq!(
        f.service
            .submit(
                &f.actor,
                target.target,
                &token,
                payload,
                &Cancellation::default()
            )
            .await
            .err()
            .map(|e| e == Error::Busy),
        Some(true)
    );
    assert_eq!(f.count("product_runs").await, 0);
    assert_eq!(inputs.retained(), (0, 0));
    f.close().await;
}

#[tokio::test]
async fn m_slot_budget_legal_small_limit_and_production_checked_arithmetic() {
    assert!(audit_room(99_996, 100_000));
    assert!(!audit_room(99_997, 100_000));
    assert!(!audit_room(u64::MAX, 100_000));
    let f = Fixture::new().await;
    let mut store = f.store.clone();
    store.audit_limit = 12;
    let target = f.target(10).await;
    let target2 = f.target(11).await;
    let target3 = f.target(12).await;
    let target4 = f.target(13).await;
    let p1 = f.proof(&target, 31).await;
    let p2 = f.proof(&target2, 32).await;
    let p3 = f.proof(&target3, 33).await;
    let p4 = f.proof(&target4, 34).await;
    let epoch = p1.identity().epoch;
    for (target, proof) in [(&target, &p1), (&target2, &p2)] {
        store
            .admit(f.actor.0, proof, target, true, &f.clock, epoch, deadline())
            .await
            .unwrap();
    }
    let (mut other, pool) = f.second_store().await;
    other.audit_limit = 12;
    let (x, y) = tokio::join!(
        store.admit(f.actor.0, &p3, &target3, true, &f.clock, epoch, deadline()),
        other.admit(f.actor.0, &p4, &target4, true, &f.clock, epoch, deadline())
    );
    assert_ne!(x.is_ok(), y.is_ok());
    assert_eq!(x.err().or_else(|| y.err()), Some(Error::Capacity));
    assert_eq!(f.count("product_audit_slots").await, 12);
    let (fact, replayed) = store
        .admit(f.actor.0, &p1, &target, true, &f.clock, epoch, deadline())
        .await
        .unwrap();
    assert!(replayed);
    assert!(
        store
            .claim(
                f.actor.0,
                &p1,
                &target,
                &fact,
                id(90),
                &f.clock,
                epoch,
                deadline()
            )
            .await
            .unwrap()
    );
    let receipt = InputReceipt {
        command: fact.command,
        run: fact.run,
        claim: id(90),
        target: target.clone(),
        owner_nonce: id(88),
    };
    store
        .report(&receipt, InputResult::Completed, &f.clock, deadline())
        .await
        .unwrap();
    assert_eq!(f.count("product_audit_slots").await, 12);
    pool.close().await;
    f.close().await;
}

#[tokio::test]
async fn m_memory_budgets_account_objects_extra_bytes_and_cleanup() {
    let inputs = MemoryInputs::default();
    assert!(inputs.reserve(16 * 1024 * 1024 + 1, 0, deadline()).is_err());
    assert!(inputs.reserve(16 * 1024 * 1024, 1, deadline()).is_err());
    let mut objects = Vec::new();
    for _ in 0..64 {
        objects.push(inputs.reserve(1, 0, deadline()).unwrap());
    }
    assert!(inputs.reserve(1, 0, deadline()).is_err());
    assert_eq!(inputs.retained(), (64, 64));
    drop(objects);
    assert_eq!(inputs.retained(), (0, 0));
    let mut objects = Vec::new();
    for _ in 0..8 {
        objects.push(inputs.reserve(16 * 1024 * 1024, 0, deadline()).unwrap());
    }
    assert!(inputs.reserve(1, 0, deadline()).is_err());
    assert_eq!(inputs.retained(), (128 * 1024 * 1024, 8));
    inputs.stop();
    assert_eq!(inputs.retained(), (0, 0));
    drop(objects);
    let mut invalid = inputs.reserve(1, 0, deadline()).unwrap();
    invalid.append(b"x").unwrap();
    assert!(invalid.finish(&Structure).is_err());
    assert_eq!(inputs.retained(), (0, 0));
    let mut incomplete = inputs.reserve(2, 0, deadline()).unwrap();
    incomplete.append(b"x").unwrap();
    assert!(incomplete.finish(&Structure).is_err());
    assert_eq!(inputs.retained(), (0, 0));
}

#[tokio::test]
async fn m_restart_marks_unknown_without_ticket_or_dispatch_slot_fabrication() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let proof = f.proof(&target, 31).await;
    let (_fact, _) = f.admit(&target, &proof).await.unwrap();
    let control = f.store.control().await.unwrap();
    let (reopened, pool) = f.second_store().await;
    assert_eq!(reopened.mark_interrupted(&f.clock).await.unwrap(), 1);
    assert!(reopened.faulted());
    assert_eq!(reopened.control().await.unwrap(), control);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM product_commands")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "unknown"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM product_runs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "unknown"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM product_resources")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "unknown"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage IN ('dispatch','reconciliation') AND occurred_at IS NOT NULL").fetch_one(&pool).await.unwrap(),0);
    assert_eq!(reopened.mark_interrupted(&f.clock).await.unwrap(), 1);
    assert!(reopened.faulted());
    pool.close().await;
    f.close().await;
}

#[tokio::test]
async fn m_issue_checkpoint_failure_unknown_and_fresh_time_do_not_return_token() {
    for fault in [
        Fault::Step(1),
        Fault::Step(2),
        Fault::DeferredCommit,
        Fault::CommitUnknown,
        Fault::CasZero,
    ] {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        f.clock.set(NOW + 50);
        *f.store.fault.lock().unwrap() = Some(fault);
        assert!(
            f.service
                .issue(
                    &f.actor,
                    target.target,
                    Sha256::digest(BODY).into(),
                    &Cancellation::default()
                )
                .await
                .is_err()
        );
        assert_eq!(f.count("product_commands").await, 0);
        assert!(f.store.faulted());
        assert_eq!(
            f.store.control().await.unwrap().1,
            if matches!(fault, Fault::CommitUnknown) {
                NOW + 50
            } else {
                NOW
            }
        );
        f.close().await;
    }
}

#[tokio::test]
async fn m_commit_cancellation_distinguishes_rollback_from_durable_unknown() {
    for fault in [Fault::CommitPending, Fault::AfterCommitPending] {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        let proof = f.proof(&target, 31).await;
        *f.store.fault.lock().unwrap() = Some(fault);
        assert!(
            timeout(Duration::from_millis(25), f.admit(&target, &proof))
                .await
                .is_err()
        );
        assert!(f.store.faulted());
        assert_eq!(
            f.count("product_commands").await,
            i64::from(matches!(fault, Fault::AfterCommitPending))
        );
        let mut connection = timeout(Duration::from_secs(2), f.pool.acquire())
            .await
            .unwrap()
            .unwrap();
        timeout(
            Duration::from_secs(2),
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *connection),
        )
        .await
        .unwrap()
        .unwrap();
        sqlx::query("ROLLBACK")
            .execute(&mut *connection)
            .await
            .unwrap();
        drop(connection);
        f.close().await;
    }
}

#[tokio::test]
async fn m_digest_fixed_array_and_signed_same_identity_conflicts() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let expected_array = format!(
        r#"["{}","{}","native-10","{}"]"#,
        id(10).encode(),
        id(2).encode(),
        hex::encode(Sha256::digest(BODY))
    );
    let intent = input_intent(f.actor.0.client_id, &target, Sha256::digest(BODY).into()).unwrap();
    assert_eq!(
        intent.payload_digest,
        <[u8; 32]>::from(Sha256::digest(expected_array.as_bytes()))
    );
    let mut composed = target.clone();
    composed.native = NativeSessionKey::from_config(Some(id(2)), "é").unwrap();
    let mut decomposed = target.clone();
    decomposed.native = NativeSessionKey::from_config(Some(id(2)), "e\u{301}").unwrap();
    assert_ne!(
        input_intent(id(1), &composed, [1; 32])
            .unwrap()
            .payload_digest,
        input_intent(id(1), &decomposed, [1; 32])
            .unwrap()
            .payload_digest
    );
    let proof = f.proof(&target, 31).await;
    let (fact, _) = f.admit(&target, &proof).await.unwrap();
    let codec = Codec::new();
    let token = codec
        .issue(
            proof.identity().epoch,
            id(31),
            &intent,
            f.clock.sample().unwrap(),
        )
        .unwrap();
    let parsed = crate::jws::ParsedJws::parse(&token, &[JwsType::ProductRequest]).unwrap();
    for field in ["expiresAt", "issuedAt", "payloadDigest"] {
        let mut claims = parsed.payload().clone();
        if field == "expiresAt" {
            claims[field] = json!(NOW + 100);
        } else if field == "issuedAt" {
            claims[field] = json!(NOW - 1);
            claims["expiresAt"] = json!(NOW + 100);
        } else {
            claims[field] = json!("a".repeat(64));
        }
        let encoded = codec
            .sign_product_request(&serde_json::to_vec(&claims).unwrap())
            .unwrap();
        let mut expected = intent.clone();
        if field == "payloadDigest" {
            expected.payload_digest = [0xaa; 32];
        }
        let different = codec
            .verify(
                &encoded,
                proof.identity().epoch,
                &expected,
                f.clock.sample().unwrap(),
            )
            .unwrap();
        assert_eq!(
            f.admit(&target, &different).await.err(),
            Some(Error::Conflict)
        );
    }
    assert_eq!(f.count("product_commands").await, 1);
    assert_eq!(
        f.store
            .get(f.actor.0, fact.command, deadline())
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Accepted
    );
    f.close().await;
}

#[tokio::test]
async fn m_public_final_expiry_actor_and_resource_fences_are_zero_send() {
    for change in ["expiry", "actor", "resource", "native-active"] {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        let token = f.token(&target).await;
        let inputs = MemoryInputs::default();
        let (fact, ticket) = f.admitted(&target, &token, &inputs).await;
        let permit = f
            .service
            .claim(&f.actor, ticket, &Cancellation::default())
            .await
            .unwrap();
        match change {
            "expiry" => f.clock.set(NOW + 86_400_000),
            "actor" => {
                sqlx::query("UPDATE clients SET status='revoked',auth_generation=4")
                    .execute(&f.pool)
                    .await
                    .unwrap();
            }
            "resource" => {
                sqlx::query("UPDATE product_resources SET revision=1")
                    .execute(&f.pool)
                    .await
                    .unwrap();
            }
            _ => {
                f.authority
                    .owner(target.target)
                    .await
                    .unwrap()
                    .lock()
                    .await
                    .observe_foreground(WorkActivity::Active)
                    .unwrap();
            }
        }
        let writer = Writer::default();
        assert!(
            f.service
                .dispatch(&f.actor, permit, &writer, &Cancellation::default())
                .await
                .is_err()
        );
        assert_eq!(writer.calls.load(Ordering::SeqCst), 0);
        assert_eq!(inputs.retained(), (0, 0));
        let status: String =
            sqlx::query_scalar("SELECT status FROM product_commands WHERE command_id=?")
                .bind(fact.command.encode())
                .fetch_one(&f.pool)
                .await
                .unwrap();
        assert!(status == "failed" || status == "expired");
        f.close().await;
    }
}

#[tokio::test]
async fn m_self_ticket_claim_allows_only_itself_and_native_active_is_not_queued() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let token = f.token(&target).await;
    let inputs = MemoryInputs::default();
    let (fact, ticket) = f.admitted(&target, &token, &inputs).await;
    f.authority
        .owner(target.target)
        .await
        .unwrap()
        .lock()
        .await
        .observe_foreground(WorkActivity::Active)
        .unwrap();
    assert!(
        f.service
            .claim(&f.actor, ticket, &Cancellation::default())
            .await
            .is_err()
    );
    assert_eq!(inputs.retained(), (0, 0));
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM product_commands WHERE command_id=?")
            .bind(fact.command.encode())
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "failed"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='dispatch' AND occurred_at IS NOT NULL").fetch_one(&f.pool).await.unwrap(),0);
    f.authority
        .owner(target.target)
        .await
        .unwrap()
        .lock()
        .await
        .observe_foreground(WorkActivity::Idle)
        .unwrap();
    let replay = f
        .service
        .submit(
            &f.actor,
            target.target,
            &token,
            f.body(&target, &inputs).await,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    assert!(matches!(replay, InputAdmission::Replay(_)));
    assert_eq!(replay.fact().status, ProductCommandStatus::Failed);
    f.close().await;
}

struct BlockingWriter {
    entered: Arc<Notify>,
    dropped: Arc<AtomicBool>,
    calls: Arc<AtomicU64>,
}
struct WriteLease {
    dropped: Arc<AtomicBool>,
}
impl Drop for WriteLease {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}
impl InputTransport for BlockingWriter {
    async fn write_once(&self, _body: &[u8]) -> Result<(), Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let _lease = WriteLease {
            dropped: self.dropped.clone(),
        };
        self.entered.notify_one();
        std::future::pending().await
    }
}
#[tokio::test]
async fn m_writer_deadline_and_cancel_drop_future_release_gate_and_keep_unknown() {
    for cancel_early in [false, true] {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        let token = f.token(&target).await;
        let inputs = MemoryInputs::default();
        let (_fact, ticket) = f.admitted(&target, &token, &inputs).await;
        let permit = f
            .service
            .claim(&f.actor, ticket, &Cancellation::default())
            .await
            .unwrap();
        let entered = Arc::new(Notify::new());
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicU64::new(0));
        let writer = BlockingWriter {
            entered: entered.clone(),
            dropped: dropped.clone(),
            calls: calls.clone(),
        };
        let actor = f.actor.clone();
        let service = f.service.clone();
        let cancel = Cancellation::default();
        let task_cancel = cancel.clone();
        let start = Instant::now();
        let task = tokio::spawn(async move {
            service
                .dispatch(&actor, permit, &writer, &task_cancel)
                .await
        });
        timeout(Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
        let mut connection = f.pool.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .unwrap();
        sqlx::query("ROLLBACK")
            .execute(&mut *connection)
            .await
            .unwrap();
        drop(connection);
        assert_eq!(inputs.retained(), (BODY.len(), 1));
        if cancel_early {
            cancel.cancel();
        }
        assert!(
            timeout(Duration::from_secs(3), task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(inputs.retained(), (0, 0));
        let held_gate = timeout(Duration::from_millis(100), f.authority.gate.lock())
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM product_commands")
                .fetch_one(&f.pool)
                .await
                .unwrap(),
            "unknown"
        );
        drop(held_gate);
        f.close().await;
    }
}

#[tokio::test]
async fn m_default_10000_commands_use_legal_runs_and_four_slots() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let epoch = f.store.control().await.unwrap().0;
    let mut transaction = f.pool.begin().await.unwrap();
    for n in 0_u64..9999 {
        let mut bytes = [220; 16];
        bytes[..8].copy_from_slice(&n.to_le_bytes());
        let key = EntityId::from_bytes(bytes).encode();
        sqlx::query("INSERT INTO product_commands VALUES(?,?,3,?,?,'session.input','session',?,?,?,?,'completed',?)").bind(&key).bind(id(1).encode()).bind(&key).bind(epoch.0.encode()).bind(target.target.encode()).bind("1".repeat(64)).bind(i64::try_from(NOW).unwrap()).bind(i64::try_from(NOW+86_400_000).unwrap()).bind(&key).execute(&mut *transaction).await.unwrap();
        sqlx::query("INSERT INTO product_runs VALUES(?,?,?,'completed')")
            .bind(&key)
            .bind(&key)
            .bind(target.target.encode())
            .execute(&mut *transaction)
            .await
            .unwrap();
        for (stage, outcome) in [
            ("admission", Some("accepted")),
            ("dispatch", Some("dispatching")),
            ("result", Some("completed")),
            ("reconciliation", None),
        ] {
            sqlx::query("INSERT INTO product_audit_slots VALUES(?,?,?,?,?,?,?)")
                .bind(&key)
                .bind(stage)
                .bind(id(1).encode())
                .bind(i64::try_from(NOW).unwrap())
                .bind(outcome.map(|_| i64::try_from(NOW).unwrap()))
                .bind(outcome.map(|_| i64::try_from(NOW + AUDIT_RETENTION_MS).unwrap()))
                .bind(outcome)
                .execute(&mut *transaction)
                .await
                .unwrap();
        }
    }
    transaction.commit().await.unwrap();
    let proof = f.proof(&target, 31).await;
    assert!(f.admit(&target, &proof).await.is_ok());
    assert_eq!(
        (
            f.count("product_commands").await,
            f.count("product_runs").await,
            f.count("product_audit_slots").await
        ),
        (10_000, 10_000, 40_000)
    );
    let other = f.target(11).await;
    assert_eq!(
        f.admit(&other, &f.proof(&other, 32).await).await.err(),
        Some(Error::Capacity)
    );
    assert!(f.admit(&target, &proof).await.unwrap().1);
    assert_eq!(f.count("product_commands").await, 10_000);
    f.close().await;
}

#[tokio::test]
async fn m_new_path_never_changes_legacy_body_or_security_metadata() {
    let f = Fixture::new().await;
    sqlx::query("INSERT INTO harnesses VALUES('old-h','old','synthetic','[]',NULL)")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces VALUES('old-w','synthetic')")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions VALUES('old-s','old-h','old-w','released',NULL,NULL,NULL)")
        .execute(&f.pool)
        .await
        .unwrap();
    let old = b"LEGACY_SYNTHETIC_BODY";
    sqlx::query("INSERT INTO content_objects VALUES('old-content','old-s',?,3,'input',?,?,?)")
        .bind(id(1).encode())
        .bind(old.as_slice())
        .bind(hex::encode(Sha256::digest(old)))
        .bind(i64::try_from(old.len()).unwrap())
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO management_commands VALUES(?,'old-request','device.revoke','old-digest','{}',9)").bind(id(1).encode()).execute(&f.pool).await.unwrap();
    sqlx::query(
        "INSERT INTO security_audit(actor_id,action,outcome,occurred_at) VALUES(?,'old','kept',1)",
    )
    .bind(id(1).encode())
    .execute(&f.pool)
    .await
    .unwrap();
    let before: Vec<u8> = sqlx::query_scalar("SELECT body FROM content_objects")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let target = f.target(10).await;
    let token = f.token(&target).await;
    let inputs = MemoryInputs::default();
    let (fact, ticket) = f.admitted(&target, &token, &inputs).await;
    let permit = f
        .service
        .claim(&f.actor, ticket, &Cancellation::default())
        .await
        .unwrap();
    let writer = Writer::default();
    let receipt = f
        .service
        .dispatch(&f.actor, permit, &writer, &Cancellation::default())
        .await
        .unwrap();
    f.service
        .report(
            &receipt,
            NativeInputReport::Unknown,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Vec<u8>>("SELECT body FROM content_objects")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        (
            f.count("content_objects").await,
            f.count("management_commands").await,
            f.count("security_audit").await
        ),
        (1, 1, 1)
    );
    for table in [
        "turns",
        "conversation_items",
        "session_commands",
        "history_content_pins",
    ] {
        assert_eq!(f.count(table).await, 0);
    }
    assert_eq!(
        f.service
            .command(
                &f.actor,
                target.target,
                fact.command,
                &Cancellation::default()
            )
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Unknown
    );
    let path = f.path.clone();
    f.close().await;
    let bytes = std::fs::read(path).unwrap();
    assert!(!bytes.windows(BODY.len()).any(|window| window == BODY));
}

#[tokio::test]
async fn m_valid_signed_proof_does_not_enable_other_actions_or_old_request_epoch() {
    use crate::domain::requests::{ProductAction, ProductTargetKind};
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let codec = Codec::new();
    let epoch = f.store.control().await.unwrap().0;
    let mut original = input_intent(id(1), &target, Sha256::digest(BODY).into()).unwrap();
    for action in [
        ProductAction::HarnessRegister,
        ProductAction::WorkspaceRegister,
        ProductAction::SessionCreate,
        ProductAction::SessionLoad,
        ProductAction::SessionCancel,
        ProductAction::SessionRelease,
        ProductAction::SessionResume,
        ProductAction::ApprovalDecide,
    ] {
        let mut intent = original.clone();
        intent.action = action;
        let time = f.clock.sample().unwrap();
        let token = codec.issue(epoch, id(31), &intent, time).unwrap();
        let proof = codec.verify(&token, epoch, &intent, time).unwrap();
        assert_eq!(
            f.store
                .admit(
                    f.actor.0,
                    &proof,
                    &target,
                    true,
                    &f.clock,
                    epoch,
                    deadline()
                )
                .await
                .err(),
            Some(Error::InvalidIdentity)
        );
    }
    original.target.kind = ProductTargetKind::Source;
    let time = f.clock.sample().unwrap();
    let token = codec.issue(epoch, id(31), &original, time).unwrap();
    let proof = codec.verify(&token, epoch, &original, time).unwrap();
    assert_eq!(
        f.store
            .admit(
                f.actor.0,
                &proof,
                &target,
                true,
                &f.clock,
                epoch,
                deadline()
            )
            .await
            .err(),
        Some(Error::InvalidIdentity)
    );
    let intent = input_intent(id(1), &target, Sha256::digest(BODY).into()).unwrap();
    let old = RequestEpoch(id(99));
    let token = codec.issue(old, id(31), &intent, time).unwrap();
    let proof = codec.verify(&token, old, &intent, time).unwrap();
    assert_eq!(
        f.store
            .admit(
                f.actor.0,
                &proof,
                &target,
                true,
                &f.clock,
                epoch,
                deadline()
            )
            .await
            .err(),
        Some(Error::EpochMismatch)
    );
    assert_eq!(f.count("product_commands").await, 0);
    f.close().await;
}

#[tokio::test]
async fn m_scoped_revoke_stop_and_old_resource_receipt_cannot_be_reused() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let body = f.body(&target, &inputs).await;
    assert!(!format!("{body:?}").contains("INDEPENDENT_BODY_MARKER"));
    inputs.revoke(id(1));
    assert_eq!(inputs.retained(), (0, 0));
    assert!(body.digest().is_err());
    drop(body);
    let body = f.body(&target, &inputs).await;
    inputs.stop();
    assert_eq!(inputs.retained(), (0, 0));
    assert!(body.digest().is_err());
    drop(body);
    let token = f.token(&target).await;
    let (fact, ticket) = f.admitted(&target, &token, &inputs).await;
    let permit = f
        .service
        .claim(&f.actor, ticket, &Cancellation::default())
        .await
        .unwrap();
    let writer = Writer::default();
    let receipt = f
        .service
        .dispatch(&f.actor, permit, &writer, &Cancellation::default())
        .await
        .unwrap();
    let mut replacement = target.clone();
    replacement.epoch.connection = HarnessConnectionEpoch(id(99));
    let mut owner = InputOwner::from_verified_source(replacement, true).unwrap();
    owner.observe_foreground(WorkActivity::Idle).unwrap();
    f.authority
        .owners
        .lock()
        .unwrap()
        .insert(target.target, Arc::new(AsyncMutex::new(owner)));
    assert_eq!(
        f.service
            .report(
                &receipt,
                NativeInputReport::Completed,
                &Cancellation::default()
            )
            .await
            .err(),
        Some(Error::InvalidTicket)
    );
    assert_eq!(
        f.store
            .get(f.actor.0, fact.command, deadline())
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Dispatching
    );
    assert_eq!(writer.calls.load(Ordering::SeqCst), 1);
    f.close().await;
}

#[tokio::test]
async fn m_startup_required_writes_commit_failure_and_unknown_never_ready() {
    for fault in (1..=6)
        .map(Fault::Step)
        .chain([Fault::DeferredCommit, Fault::CommitUnknown])
    {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        let proof = f.proof(&target, 31).await;
        f.admit(&target, &proof).await.unwrap();
        let (store, pool) = f.second_store().await;
        *store.fault.lock().unwrap() = Some(fault);
        assert!(store.mark_interrupted(&f.clock).await.is_err());
        assert!(store.faulted());
        let status: String = sqlx::query_scalar("SELECT status FROM product_commands")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            status,
            if matches!(fault, Fault::CommitUnknown) {
                "unknown"
            } else {
                "accepted"
            }
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='reconciliation' AND occurred_at IS NOT NULL").fetch_one(&pool).await.unwrap(),0);
        pool.close().await;
        f.close().await;
    }
}

#[tokio::test]
async fn m_retention_upper_bound_checked_without_partial_admission_or_false_result() {
    use crate::domain::requests::{MAX_REQUEST_TIME, REQUEST_VALIDITY_MS};
    let f = Fixture::new().await;
    let target = f.target(10).await;
    f.clock.set(MAX_REQUEST_TIME - REQUEST_VALIDITY_MS);
    let proof = f.proof(&target, 31).await;
    assert_eq!(
        f.admit(&target, &proof).await.err(),
        Some(Error::ClockUnreliable)
    );
    assert_eq!(
        (
            f.count("product_commands").await,
            f.count("product_runs").await,
            f.count("product_audit_slots").await
        ),
        (0, 0, 0)
    );
    f.close().await;
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let proof = f.proof(&target, 31).await;
    let (fact, _) = f.admit(&target, &proof).await.unwrap();
    let receipt = f.private_claim(&target, &proof, &fact).await;
    f.clock.set(MAX_REQUEST_TIME - 1);
    assert_eq!(
        f.store
            .report(&receipt, InputResult::Completed, &f.clock, deadline())
            .await
            .err(),
        Some(Error::ClockUnreliable)
    );
    assert_eq!(
        f.store
            .get(f.actor.0, fact.command, deadline())
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Dispatching
    );
    assert!(f.store.faulted());
    f.close().await;
}

#[tokio::test]
async fn m_gate_owner_and_sql_waits_obey_absolute_five_second_limits() {
    for boundary in ["gate", "owner", "sql"] {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        let inputs = MemoryInputs::default();
        let gate = if boundary == "gate" {
            Some(f.authority.gate.clone().lock_owned().await)
        } else {
            None
        };
        let owner = if boundary == "owner" {
            Some(
                f.authority
                    .owner(target.target)
                    .await
                    .unwrap()
                    .lock_owned()
                    .await,
            )
        } else {
            None
        };
        let mut lock = if boundary == "sql" {
            let mut connection = f.pool.acquire().await.unwrap();
            sqlx::query("BEGIN IMMEDIATE")
                .execute(&mut *connection)
                .await
                .unwrap();
            Some(connection)
        } else {
            None
        };
        let start = Instant::now();
        if boundary == "sql" {
            assert!(
                f.service
                    .issue(
                        &f.actor,
                        target.target,
                        Sha256::digest(BODY).into(),
                        &Cancellation::default()
                    )
                    .await
                    .is_err()
            );
        } else {
            assert!(
                f.service
                    .prepare_input(
                        &f.actor,
                        target.target,
                        &inputs,
                        BODY.len(),
                        0,
                        &Cancellation::default()
                    )
                    .await
                    .is_err()
            );
        }
        assert!(start.elapsed() >= Duration::from_millis(4500));
        assert!(start.elapsed() < Duration::from_secs(7));
        drop(gate);
        drop(owner);
        if let Some(connection) = &mut lock {
            sqlx::query("ROLLBACK")
                .execute(&mut **connection)
                .await
                .unwrap();
        }
        drop(lock);
        assert_eq!(f.count("product_commands").await, 0);
        assert_eq!(inputs.retained(), (0, 0));
        f.close().await;
    }
}

#[tokio::test]
async fn m_dropped_new_ticket_cannot_be_reconstructed_or_keep_plaintext() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let token = f.token(&target).await;
    let inputs = MemoryInputs::default();
    let (fact, ticket) = f.admitted(&target, &token, &inputs).await;
    drop(ticket);
    assert_eq!(inputs.retained(), (0, 0));
    assert!(f.store.faulted());
    let view = f
        .service
        .command(
            &f.actor,
            target.target,
            fact.command,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(view.status, ProductCommandStatus::Accepted);
    let body = f.body(&target, &inputs).await;
    assert!(
        f.service
            .submit(
                &f.actor,
                target.target,
                &token,
                body,
                &Cancellation::default()
            )
            .await
            .is_err()
    );
    assert_eq!(inputs.retained(), (0, 0));
    assert_eq!(f.count("product_commands").await, 1);
    f.close().await;
}

#[tokio::test]
async fn m_unknown_result_is_not_rewritten_by_later_completed_observation() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let token = f.token(&target).await;
    let inputs = MemoryInputs::default();
    let (_fact, ticket) = f.admitted(&target, &token, &inputs).await;
    let permit = f
        .service
        .claim(&f.actor, ticket, &Cancellation::default())
        .await
        .unwrap();
    let writer = Writer::default();
    let receipt = f
        .service
        .dispatch(&f.actor, permit, &writer, &Cancellation::default())
        .await
        .unwrap();
    f.service
        .report(
            &receipt,
            NativeInputReport::Unknown,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        f.service
            .report(
                &receipt,
                NativeInputReport::Completed,
                &Cancellation::default()
            )
            .await
            .err(),
        Some(Error::Conflict)
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT outcome FROM product_audit_slots WHERE stage='result'"
        )
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        "unknown"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='reconciliation' AND occurred_at IS NOT NULL").fetch_one(&f.pool).await.unwrap(),0);
    assert_eq!(writer.calls.load(Ordering::SeqCst), 1);
    f.close().await;
}
