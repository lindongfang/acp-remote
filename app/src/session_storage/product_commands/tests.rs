//! Development tests use disposable SQLite files, a synthetic ES256 key and trusted local ports.
use super::*;
use crate::{
    adapters::product_requests::{self as signed, ProductRequestSigner},
    application::product_commands::{
        Cancellation, InputAdmission, InputAuthority, InputOwner, InputStructureValidator,
        InputTransport, MemoryInputs, NativeInputReport, ProductInputService, RequestIdentityPort,
        TemporaryInput,
    },
    domain::{
        activity::WorkActivity,
        native::{
            HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch,
        },
        product_commands::{AUDIT_RETENTION_MS, input_intent},
        requests::{RequestClock, RequestIdentityError, RequestIntent},
    },
    jws::{self, JwsError, JwsType},
};
use p256::ecdsa::SigningKey;
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqlitePoolOptions;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, atomic::AtomicU64},
    time::Duration as StdDuration,
};
use tokio::sync::Mutex as AsyncMutex;

fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
#[derive(Clone)]
struct Clock {
    clock: Arc<Mutex<RequestClock>>,
    wall: Arc<AtomicU64>,
}
impl Clock {
    fn new() -> Self {
        Self {
            clock: Arc::new(Mutex::new(RequestClock::new(0, 1000).unwrap())),
            wall: Arc::new(AtomicU64::new(1000)),
        }
    }
    fn set(&self, n: u64) {
        self.wall.store(n, Ordering::SeqCst);
    }
}
impl TimeSource for Clock {
    fn sample(&self) -> Result<RequestTime, Error> {
        let n = self.wall.load(Ordering::SeqCst);
        self.clock
            .lock()
            .unwrap()
            .observe(
                Some(n),
                Some(StdDuration::from_millis(n.saturating_sub(1000))),
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
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
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
    fn sign_product_request(&self, payload: &[u8]) -> Result<String, JwsError> {
        jws::sign(JwsType::ProductRequest, payload, &self.key)
    }
}
fn identity_error(e: RequestIdentityError) -> Error {
    match e {
        RequestIdentityError::Expired => Error::Expired,
        RequestIdentityError::ClockUnreliable => Error::ClockUnreliable,
        RequestIdentityError::EpochMismatch => Error::EpochMismatch,
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
        signed::issue(self, epoch, request, intent, time)
            .map(|s| s.compact().to_owned())
            .map_err(identity_error)
    }
    fn verify(
        &self,
        compact: &str,
        epoch: RequestEpoch,
        intent: &RequestIntent,
        time: RequestTime,
    ) -> Result<VerifiedProductRequest, Error> {
        signed::verify(compact, &self.public(), epoch, intent, time).map_err(identity_error)
    }
}
#[derive(Clone)]
struct Actor(ActorContext);
#[derive(Clone)]
struct Authority {
    pool: SqlitePool,
    gate: Arc<AsyncMutex<()>>,
    owners: Arc<Mutex<HashMap<EntityId, Arc<AsyncMutex<InputOwner>>>>>,
    pause_refresh: Arc<Mutex<Option<Arc<RefreshPause>>>>,
}
#[derive(Default)]
struct RefreshPause {
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
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
        require_actor(&mut *self.pool.acquire().await?, actor.0).await?;
        Ok(actor.0)
    }
    async fn refresh_owner(&self, _owner: &mut InputOwner) -> Result<(), Error> {
        let pause = self.pause_refresh.lock().unwrap().take();
        if let Some(pause) = pause {
            pause.entered.notify_one();
            pause.release.notified().await;
        }
        Ok(())
    }
    fn owner(
        &self,
        target: EntityId,
    ) -> impl std::future::Future<Output = Result<Arc<AsyncMutex<InputOwner>>, Error>> + Send {
        std::future::ready(
            self.owners
                .lock()
                .unwrap()
                .get(&target)
                .cloned()
                .ok_or(Error::InvalidTarget),
        )
    }
}
struct Area {
    path: PathBuf,
    root: PathBuf,
}
impl Area {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap();
        let path = root.join(format!(
            "acpr-command-test-{}",
            crate::random::hex(12).unwrap()
        ));
        std::fs::create_dir(&path).unwrap();
        Self { path, root }
    }
}
impl Drop for Area {
    fn drop(&mut self) {
        if self.path.parent() == Some(self.root.as_path())
            && self
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("acpr-command-test-")
        {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
struct Fixture {
    _area: Arc<Area>,
    storage: crate::sqlite_fixture_scope::FixtureStorage,
    storage_scope: crate::sqlite_fixture_scope::SqliteFixtureScope,
    raw_scope: crate::sqlite_fixture_scope::SqliteFixtureScope,
    pool: SqlitePool,
    store: ProductStore,
    clock: Clock,
    epoch: RequestEpoch,
    authority: Authority,
    actor: Actor,
}
impl Fixture {
    async fn new() -> Self {
        let area = Arc::new(Area::new());
        let path = area.path.join("commands.sqlite3");
        let storage_scope =
            crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&path, area.clone());
        let storage = storage_scope.open().await.unwrap();
        let raw_scope =
            crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&path, area.clone());
        raw_scope.start_raw().await;
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(
                raw_scope
                    .options()
                    .foreign_keys(true)
                    .busy_timeout(StdDuration::from_secs(5)),
            )
            .await
            .unwrap();
        sqlx::query("INSERT INTO clients VALUES(?,X'01','synthetic','trusted',7,0,NULL)")
            .bind(id(1).encode())
            .execute(&pool)
            .await
            .unwrap();
        let store = storage.product_commands();
        let (epoch, _) = store.control().await.unwrap();
        let clock = Clock::new();
        let authority = Authority {
            pool: pool.clone(),
            gate: Arc::default(),
            owners: Arc::default(),
            pause_refresh: Arc::default(),
        };
        Self {
            _area: area,
            storage,
            storage_scope,
            raw_scope,
            pool,
            store,
            clock,
            epoch,
            authority,
            actor: Actor(ActorContext {
                client_id: id(1),
                auth_generation: 7,
            }),
        }
    }
    async fn target(&self, n: u8) -> InputTarget {
        let target = InputTarget {
            target: id(n),
            native: NativeSessionKey::from_config(Some(id(2)), &format!("native-{n}")).unwrap(),
            epoch: ResourceEpoch {
                runtime: RuntimeEpoch(id(3)),
                connection: HarnessConnectionEpoch(id(n.wrapping_add(80))),
                load: LoadEpoch(id(n.wrapping_add(100))),
            },
            instance: id(4),
            revision: 0,
        };
        sqlx::query("INSERT OR IGNORE INTO product_sources VALUES(?,'synthetic source')")
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
    fn proof(&self, target: &InputTarget, request: u8) -> VerifiedProductRequest {
        let intent = input_intent(
            self.actor.0.client_id,
            target,
            Sha256::digest(b"synthetic input").into(),
        )
        .unwrap();
        let time = self.clock.sample().unwrap();
        let codec = Codec::new();
        let token = codec.issue(self.epoch, id(request), &intent, time).unwrap();
        codec.verify(&token, self.epoch, &intent, time).unwrap()
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
                self.epoch,
                deadline(),
            )
            .await
    }
    async fn service(&self) -> ProductInputService<Authority, Codec, Clock, ProductStore> {
        ProductInputService::start(
            self.store.clone(),
            self.authority.clone(),
            Codec::new(),
            self.clock.clone(),
        )
        .await
        .unwrap()
    }
    async fn close(self) {
        self.pool.close().await;
        self.raw_scope.stop_and_observe().await;
        self.storage.close().await;
        self.storage_scope.stop_and_observe().await;
    }
}
struct Structure;
impl InputStructureValidator for Structure {
    fn validate(&self, body: &[u8]) -> Result<(), Error> {
        if body.starts_with(b"synthetic") {
            Ok(())
        } else {
            Err(Error::InvalidInput)
        }
    }
}
fn body(inputs: &MemoryInputs) -> TemporaryInput {
    let mut upload = inputs.reserve(15, 0, deadline()).unwrap();
    upload.append(b"synthetic input").unwrap();
    upload.finish(&Structure).unwrap()
}
#[derive(Default)]
struct Writer {
    calls: AtomicU64,
    blocked: bool,
}
impl InputTransport for Writer {
    async fn write_once(&self, body: &[u8]) -> Result<(), Error> {
        assert_eq!(body, b"synthetic input");
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.blocked {
            std::future::pending().await
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn metadata_admission_replay_conflict_and_no_body_tables() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let proof = f.proof(&target, 20);
    let (first, replayed) = f.admit(&target, &proof).await.unwrap();
    assert!(!replayed);
    let (again, replayed) = f.admit(&target, &proof).await.unwrap();
    assert_eq!(first, again);
    assert!(replayed);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_commands")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_audit_slots")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM product_audit_slots WHERE occurred_at IS NOT NULL"
        )
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
    for table in [
        "turns",
        "conversation_items",
        "content_objects",
        "session_commands",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&f.pool)
                .await
                .unwrap(),
            0
        );
    }
    let mut changed = proof.identity().clone();
    changed.intent.payload_digest = [55; 32];
    assert_eq!(
        f.admit(&target, &VerifiedProductRequest(changed))
            .await
            .err(),
        Some(Error::Conflict)
    );
    let row = sqlx::query(
        "SELECT occurred_at,retain_until FROM product_audit_slots WHERE stage='admission'",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        row.get::<i64, _>("retain_until") - row.get::<i64, _>("occurred_at"),
        i64::try_from(AUDIT_RETENTION_MS).unwrap()
    );
    f.close().await;
}
#[tokio::test]
async fn two_pools_duplicate_and_same_target_compete_in_real_sqlite() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let p1 = f.proof(&target, 20);
    let p2 = f.proof(&target, 20);
    let mut other = ProductStore::new(f.pool.clone());
    other.faulted = f.store.faulted.clone();
    let (a, b) = tokio::join!(
        f.admit(&target, &p1),
        other.admit(f.actor.0, &p2, &target, true, &f.clock, f.epoch, deadline())
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.0.command, b.0.command);
    assert_ne!(a.1, b.1);
    let second = f.proof(&target, 21);
    assert_eq!(
        other
            .admit(
                f.actor.0,
                &second,
                &target,
                true,
                &f.clock,
                f.epoch,
                deadline()
            )
            .await
            .err(),
        Some(Error::Busy)
    );
    let next = f.target(11).await;
    assert!(f.admit(&next, &f.proof(&next, 22)).await.is_ok());
    f.close().await;
}
#[tokio::test]
async fn every_admission_write_failure_rolls_back_and_commit_unknown_stays_durable() {
    for fault in (1..=9)
        .map(Fault::Step)
        .chain([Fault::DeferredCommit, Fault::CommitUnknown])
    {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        let proof = f.proof(&target, 20);
        *f.store.fault.lock().unwrap() = Some(fault);
        assert!(f.admit(&target, &proof).await.is_err());
        assert!(f.store.faulted());
        assert!(f.storage.product_commands().faulted());
        let commands: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_commands")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(
            commands,
            i64::from(matches!(fault, Fault::CommitUnknown)),
            "{fault:?}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_audit_slots")
                .fetch_one(&f.pool)
                .await
                .unwrap(),
            commands * 4
        );
        f.close().await;
    }
}
#[tokio::test]
async fn expiry_precedes_dedup_and_claim_expiry_does_not_fill_dispatch() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let proof = f.proof(&target, 20);
    let (fact, _) = f.admit(&target, &proof).await.unwrap();
    f.clock.set(proof.identity().expires_at);
    assert_eq!(f.admit(&target, &proof).await.err(), Some(Error::Expired));
    assert_eq!(
        f.store
            .claim(
                f.actor.0,
                &proof,
                &target,
                &fact,
                id(30),
                &f.clock,
                f.epoch,
                deadline()
            )
            .await
            .err(),
        Some(Error::Expired)
    );
    assert_eq!(
        f.store
            .get(f.actor.0, fact.command, deadline())
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Expired
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='dispatch' AND occurred_at IS NOT NULL").fetch_one(&f.pool).await.unwrap(),0);
    f.close().await;
}
#[tokio::test]
async fn floor_cas_uses_database_value_and_samples_after_begin_wait() {
    let f = Fixture::new().await;
    let mut lock = f.pool.acquire().await.unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *lock)
        .await
        .unwrap();
    {
        let checkpoint = f.store.checkpoint(f.actor.0, f.epoch, &f.clock, deadline());
        tokio::pin!(checkpoint);
        tokio::select! {r=&mut checkpoint=>panic!("checkpoint skipped lock: {r:?}"),()=tokio::time::sleep(Duration::from_millis(30))=>{}}
        f.clock.set(2000);
        sqlx::query("UPDATE product_control SET time_floor_ms=1500")
            .execute(&mut *lock)
            .await
            .unwrap();
        sqlx::query("COMMIT").execute(&mut *lock).await.unwrap();
        drop(lock);
        assert_eq!(checkpoint.await.unwrap().millis(), 2000);
    }
    f.clock.set(3000);
    assert_eq!(
        f.store
            .checkpoint(f.actor.0, f.epoch, &f.clock, deadline())
            .await
            .unwrap()
            .millis(),
        3000
    );
    assert_eq!(f.store.control().await.unwrap().1, 3000);
    f.close().await;
}
#[tokio::test]
async fn protected_command_and_four_slot_limits_are_independent() {
    assert!(audit_room(99_996, AUDIT_LIMIT));
    assert!(!audit_room(99_997, AUDIT_LIMIT));
    assert!(!audit_room(u64::MAX, u64::MAX));
    let f = Fixture::new().await;
    let mut store = f.store.clone();
    store.audit_limit = 12;
    for n in 10..13 {
        let target = f.target(n).await;
        store
            .admit(
                f.actor.0,
                &f.proof(&target, n + 20),
                &target,
                true,
                &f.clock,
                f.epoch,
                deadline(),
            )
            .await
            .unwrap();
    }
    let target = f.target(13).await;
    assert_eq!(
        store
            .admit(
                f.actor.0,
                &f.proof(&target, 33),
                &target,
                true,
                &f.clock,
                f.epoch,
                deadline()
            )
            .await
            .err(),
        Some(Error::Capacity)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_audit_slots")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        12
    );
    let mut tx = f.pool.begin().await.unwrap();
    for n in 0_u64..9996 {
        let raw = n.to_le_bytes();
        let mut bytes = [222; 16];
        bytes[..8].copy_from_slice(&raw);
        let command = EntityId::from_bytes(bytes).encode();
        // Synthetic historical completed inputs: every command has its Run and four legal stages.
        // They are still protected by the signed 24-hour deadline; no real transport is implied.
        sqlx::query("INSERT INTO product_commands VALUES(?,?,7,?,?,'session.input','session',?,?,1000,86401000,'completed',?)").bind(&command).bind(f.actor.0.client_id.encode()).bind(&command).bind(f.epoch.0.encode()).bind(target.target.encode()).bind("1".repeat(64)).bind(&command).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO product_runs VALUES(?,?,?,'completed')")
            .bind(&command)
            .bind(&command)
            .bind(target.target.encode())
            .execute(&mut *tx)
            .await
            .unwrap();
        for (stage, outcome) in [
            ("admission", Some("accepted")),
            ("dispatch", Some("dispatching")),
            ("result", Some("completed")),
            ("reconciliation", None),
        ] {
            sqlx::query("INSERT INTO product_audit_slots VALUES(?,?,?,1000,?,?,?)")
                .bind(&command)
                .bind(stage)
                .bind(f.actor.0.client_id.encode())
                .bind(outcome.map(|_| 1000))
                .bind(outcome.map(|_| 7_776_001_000_i64))
                .bind(outcome)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
    }
    tx.commit().await.unwrap();
    assert!(f.admit(&target, &f.proof(&target, 34)).await.is_ok());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_commands")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        10_000
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_audit_slots")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        40_000
    );
    let next = f.target(14).await;
    assert_eq!(
        f.admit(&next, &f.proof(&next, 35)).await.err(),
        Some(Error::Capacity)
    );
    f.close().await;
}
#[tokio::test]
async fn full_service_owns_ticket_claim_dispatch_and_native_completion_separately() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, payload.digest().unwrap(), &cancel)
        .await
        .unwrap();
    let admission = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap();
    let fact = admission.fact().clone();
    let InputAdmission::New { ticket, .. } = admission else {
        panic!("new ticket");
    };
    let replay = service
        .submit(&f.actor, target.target, &token, body(&inputs), &cancel)
        .await
        .unwrap();
    assert!(matches!(replay, InputAdmission::Replay(_)));
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    let writer = Writer::default();
    let receipt = service
        .dispatch(&f.actor, permit, &writer, &cancel)
        .await
        .unwrap();
    assert_eq!(writer.calls.load(Ordering::SeqCst), 1);
    assert_eq!(inputs.retained(), (0, 0));
    service
        .report(&receipt, NativeInputReport::TransportWritten, &cancel)
        .await
        .unwrap();
    assert_eq!(
        service
            .command(&f.actor, target.target, fact.command, &cancel)
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Dispatching
    );
    service
        .report(&receipt, NativeInputReport::Inserted, &cancel)
        .await
        .unwrap();
    service
        .report(&receipt, NativeInputReport::Completed, &cancel)
        .await
        .unwrap();
    f.clock.set(2000);
    service
        .report(&receipt, NativeInputReport::Completed, &cancel)
        .await
        .unwrap();
    assert_eq!(
        service
            .command(&f.actor, target.target, fact.command, &cancel)
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Completed
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='reconciliation' AND occurred_at IS NOT NULL").fetch_one(&f.pool).await.unwrap(),0);
    f.close().await;
}
#[tokio::test]
async fn spontaneous_native_activity_after_admission_is_zero_send_failure() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, payload.digest().unwrap(), &cancel)
        .await
        .unwrap();
    let InputAdmission::New { fact, ticket } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    f.authority
        .owner(target.target)
        .await
        .unwrap()
        .lock()
        .await
        .observe_foreground(WorkActivity::Active)
        .unwrap();
    assert!(matches!(
        service.claim(&f.actor, ticket, &cancel).await,
        Err(Error::Busy)
    ));
    assert_eq!(inputs.retained(), (0, 0));
    assert_eq!(
        service
            .command(&f.actor, target.target, fact.command, &cancel)
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Failed
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='dispatch' AND occurred_at IS NOT NULL").fetch_one(&f.pool).await.unwrap(),0);
    f.close().await;
}
#[tokio::test]
async fn writer_deadline_releases_actor_gate_and_marks_unknown() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, payload.digest().unwrap(), &cancel)
        .await
        .unwrap();
    let InputAdmission::New { fact, ticket } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    let writer = Writer {
        blocked: true,
        ..Writer::default()
    };
    assert!(matches!(
        service.dispatch(&f.actor, permit, &writer, &cancel).await,
        Err(Error::Cancelled)
    ));
    assert_eq!(inputs.retained(), (0, 0));
    let gate = tokio::time::timeout(Duration::from_millis(100), f.authority.gate.lock())
        .await
        .unwrap();
    drop(gate);
    assert_eq!(writer.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        service
            .command(&f.actor, target.target, fact.command, &cancel)
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Unknown
    );
    f.close().await;
}
#[tokio::test]
async fn final_fence_epoch_change_prevents_transport_and_records_zero_send() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, payload.digest().unwrap(), &cancel)
        .await
        .unwrap();
    let InputAdmission::New { ticket, .. } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    sqlx::query("UPDATE product_control SET request_epoch=?")
        .bind(id(250).encode())
        .execute(&f.pool)
        .await
        .unwrap();
    let writer = Writer::default();
    assert!(matches!(
        service.dispatch(&f.actor, permit, &writer, &cancel).await,
        Err(Error::EpochMismatch)
    ));
    assert_eq!(writer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(inputs.retained(), (0, 0));
    f.close().await;
}
#[tokio::test]
async fn startup_never_reconstructs_tickets_or_dispatch_and_preserves_epoch_floor() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let proof = f.proof(&target, 20);
    let (fact, _) = f.admit(&target, &proof).await.unwrap();
    let before = f.store.control().await.unwrap();
    assert_eq!(f.store.mark_interrupted(&f.clock).await.unwrap(), 1);
    assert_eq!(f.store.control().await.unwrap(), before);
    assert_eq!(
        f.store
            .get(f.actor.0, fact.command, deadline())
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Unknown
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM product_resources")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "unknown"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage IN ('dispatch','reconciliation') AND occurred_at IS NOT NULL").fetch_one(&f.pool).await.unwrap(),0);
    f.close().await;
}
#[tokio::test]
async fn memory_reservations_expiry_revoke_shutdown_and_borrowed_storage_are_bounded() {
    let inputs = MemoryInputs::default();
    assert!(inputs.reserve(16 * 1024 * 1024 + 1, 0, deadline()).is_err());
    let mut held = Vec::new();
    for _ in 0..8 {
        held.push(inputs.reserve(16 * 1024 * 1024, 0, deadline()).unwrap());
    }
    assert!(inputs.reserve(1, 0, deadline()).is_err());
    drop(held);
    assert_eq!(inputs.retained(), (0, 0));
    let mut held = Vec::new();
    for _ in 0..64 {
        held.push(inputs.reserve(0, 0, deadline()).unwrap());
    }
    assert!(inputs.reserve(0, 0, deadline()).is_err());
    inputs.stop();
    assert_eq!(inputs.retained(), (0, 0));
    drop(held);
    let mut expiring = inputs
        .reserve(15, 0, Instant::now() + Duration::from_millis(20))
        .unwrap();
    expiring.append(b"synthetic input").unwrap();
    let retained = expiring.finish(&Structure).unwrap();
    let supervisor = inputs.supervise();
    tokio::time::sleep(Duration::from_millis(130)).await;
    assert!(retained.digest().is_err());
    assert_eq!(inputs.retained(), (0, 0));
    supervisor.shutdown().await;
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let mut upload = service
        .prepare_input(
            &f.actor,
            target.target,
            &inputs,
            15,
            0,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    upload.append(b"synthetic input").unwrap();
    let payload = upload.finish(&Structure).unwrap();
    inputs.revoke(id(1));
    assert_eq!(inputs.retained(), (0, 0));
    assert!(payload.digest().is_err());
    f.close().await;
}

#[tokio::test]
async fn checkpoint_faults_and_cancelled_commit_windows_never_qualify() {
    for fault in [
        Fault::Step(1),
        Fault::Step(2),
        Fault::DeferredCommit,
        Fault::CommitUnknown,
        Fault::CasZero,
        Fault::CommitPending,
        Fault::AfterCommitPending,
    ] {
        let f = Fixture::new().await;
        f.clock.set(2000);
        *f.store.fault.lock().unwrap() = Some(fault);
        let fault_deadline = if matches!(fault, Fault::CommitPending | Fault::AfterCommitPending) {
            Instant::now() + Duration::from_secs(1)
        } else {
            deadline()
        };
        let result = f
            .store
            .checkpoint(f.actor.0, f.epoch, &f.clock, fault_deadline)
            .await;
        assert!(result.is_err(), "{fault:?}");
        assert!(f.store.faulted());
        let floor: i64 = sqlx::query_scalar("SELECT time_floor_ms FROM product_control")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(
            floor,
            if matches!(fault, Fault::CommitUnknown | Fault::AfterCommitPending) {
                2000
            } else {
                0
            },
            "{fault:?}"
        );
        f.close().await;
    }
}
#[tokio::test]
async fn claim_write_commit_and_cancellation_faults_never_return_second_qualification() {
    for fault in (1..=5).map(Fault::Step).chain([
        Fault::DeferredCommit,
        Fault::CommitUnknown,
        Fault::CommitPending,
        Fault::AfterCommitPending,
        Fault::CasZero,
    ]) {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        let proof = f.proof(&target, 20);
        let (fact, _) = f.admit(&target, &proof).await.unwrap();
        *f.store.fault.lock().unwrap() = Some(fault);
        let fault_deadline = if matches!(fault, Fault::CommitPending | Fault::AfterCommitPending) {
            Instant::now() + Duration::from_secs(1)
        } else {
            deadline()
        };
        let result = f
            .store
            .claim(
                f.actor.0,
                &proof,
                &target,
                &fact,
                id(30),
                &f.clock,
                f.epoch,
                fault_deadline,
            )
            .await;
        assert!(result.is_err(), "{fault:?}");
        assert!(f.store.faulted());
        let status: String = sqlx::query_scalar("SELECT status FROM product_commands")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        let committed = matches!(fault, Fault::CommitUnknown | Fault::AfterCommitPending);
        assert_eq!(
            status,
            if committed { "dispatching" } else { "accepted" },
            "{fault:?}"
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='dispatch' AND occurred_at IS NOT NULL").fetch_one(&f.pool).await.unwrap(),i64::from(committed));
        assert!(
            f.store
                .claim(
                    f.actor.0,
                    &proof,
                    &target,
                    &fact,
                    id(31),
                    &f.clock,
                    f.epoch,
                    deadline()
                )
                .await
                .is_err()
        );
        f.close().await;
    }
}
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "The complete result/fault matrix keeps each real SQLite observation beside its injection"
)]
async fn result_write_faults_preserve_observation_and_never_reset_dispatch() {
    for result in [
        InputResult::Completed,
        InputResult::Rejected,
        InputResult::Unknown,
    ] {
        for fault in (1..=4).map(Fault::Step).chain([
            Fault::DeferredCommit,
            Fault::CommitUnknown,
            Fault::CommitPending,
            Fault::AfterCommitPending,
        ]) {
            let f = Fixture::new().await;
            let target = f.target(10).await;
            let proof = f.proof(&target, 20);
            let (fact, _) = f.admit(&target, &proof).await.unwrap();
            assert!(
                f.store
                    .claim(
                        f.actor.0,
                        &proof,
                        &target,
                        &fact,
                        id(30),
                        &f.clock,
                        f.epoch,
                        deadline()
                    )
                    .await
                    .unwrap()
            );
            let receipt = InputReceipt {
                command: fact.command,
                run: fact.run,
                claim: id(30),
                target,
                owner_nonce: id(40),
            };
            *f.store.fault.lock().unwrap() = Some(fault);
            let fault_deadline =
                if matches!(fault, Fault::CommitPending | Fault::AfterCommitPending) {
                    Instant::now() + Duration::from_secs(1)
                } else {
                    deadline()
                };
            assert!(
                f.store
                    .report(&receipt, result, &f.clock, fault_deadline)
                    .await
                    .is_err()
            );
            assert!(f.store.faulted());
            let status: String = sqlx::query_scalar("SELECT status FROM product_commands")
                .fetch_one(&f.pool)
                .await
                .unwrap();
            assert_eq!(
                status,
                if matches!(fault, Fault::CommitUnknown | Fault::AfterCommitPending) {
                    result.labels().0
                } else {
                    "dispatching"
                },
                "{fault:?}"
            );
            // A retry only confirms the same stored observation. It cannot create or consume input.
            f.store
                .report(&receipt, result, &f.clock, deadline())
                .await
                .unwrap();
            let occurred: i64 = sqlx::query_scalar(
                "SELECT occurred_at FROM product_audit_slots WHERE stage='result'",
            )
            .fetch_one(&f.pool)
            .await
            .unwrap();
            f.clock.set(2000);
            f.store
                .report(&receipt, result, &f.clock, deadline())
                .await
                .unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT occurred_at FROM product_audit_slots WHERE stage='result'"
                )
                .fetch_one(&f.pool)
                .await
                .unwrap(),
                occurred
            );
            assert_eq!(
                f.store
                    .report(
                        &receipt,
                        if result == InputResult::Unknown {
                            InputResult::Completed
                        } else {
                            InputResult::Unknown
                        },
                        &f.clock,
                        deadline()
                    )
                    .await
                    .err(),
                Some(Error::Conflict)
            );
            f.close().await;
        }
    }
}
#[tokio::test]
async fn startup_marking_faults_are_atomic_and_cannot_release_unknown_ownership() {
    for fault in (1..=6)
        .map(Fault::Step)
        .chain([Fault::DeferredCommit, Fault::CommitUnknown])
    {
        let f = Fixture::new().await;
        let target = f.target(10).await;
        f.admit(&target, &f.proof(&target, 20)).await.unwrap();
        *f.store.fault.lock().unwrap() = Some(fault);
        assert!(
            f.store.mark_interrupted(&f.clock).await.is_err(),
            "{fault:?}"
        );
        assert!(f.store.faulted());
        let status: String = sqlx::query_scalar("SELECT status FROM product_commands")
            .fetch_one(&f.pool)
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
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM product_audit_slots WHERE stage='reconciliation' AND occurred_at IS NOT NULL").fetch_one(&f.pool).await.unwrap(),0);
        f.close().await;
    }
}
#[tokio::test]
async fn default_command_limit_and_last_four_test_slots_compete_atomically() {
    let f = Fixture::new().await;
    let mut first = f.store.clone();
    first.audit_limit = 12;
    let mut second = ProductStore::new(f.pool.clone());
    second.audit_limit = 12;
    second.faulted = first.faulted.clone();
    for n in [10, 11] {
        let target = f.target(n).await;
        first
            .admit(
                f.actor.0,
                &f.proof(&target, n + 20),
                &target,
                true,
                &f.clock,
                f.epoch,
                deadline(),
            )
            .await
            .unwrap();
    }
    let t1 = f.target(12).await;
    let t2 = f.target(13).await;
    let p1 = f.proof(&t1, 32);
    let p2 = f.proof(&t2, 33);
    let (a, b) = tokio::join!(
        first.admit(f.actor.0, &p1, &t1, true, &f.clock, f.epoch, deadline()),
        second.admit(f.actor.0, &p2, &t2, true, &f.clock, f.epoch, deadline())
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert!(matches!(a, Err(Error::Capacity)) || matches!(b, Err(Error::Capacity)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_audit_slots")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        12
    );
    f.close().await;
}
#[tokio::test]
async fn sql_actor_resource_and_closed_action_checks_fail_without_metadata() {
    let f = Fixture::new().await;
    let target = f.target(10).await;
    let proof = f.proof(&target, 20);
    for generation in [-1, 6, 9_007_199_254_740_992] {
        let actor = ActorContext {
            auth_generation: generation,
            ..f.actor.0
        };
        assert_eq!(
            f.store
                .admit(actor, &proof, &target, true, &f.clock, f.epoch, deadline())
                .await
                .err(),
            Some(Error::InvalidActor)
        );
    }
    for result in ["active", "unknown"] {
        let available = result == "idle";
        assert_eq!(
            f.store
                .admit(
                    f.actor.0,
                    &proof,
                    &target,
                    available,
                    &f.clock,
                    f.epoch,
                    deadline()
                )
                .await
                .err(),
            Some(Error::Busy)
        );
    }
    let mut wrong = target.clone();
    wrong.revision = 1;
    assert_eq!(
        f.admit(&wrong, &proof).await.err(),
        Some(Error::InvalidTarget)
    );
    let mut wrong = target.clone();
    wrong.epoch.load = LoadEpoch(id(250));
    assert_eq!(
        f.admit(&wrong, &proof).await.err(),
        Some(Error::InvalidTarget)
    );
    let mut unsupported = proof.identity().clone();
    unsupported.intent.action = ProductAction::SessionCreate;
    assert_eq!(
        f.admit(&target, &VerifiedProductRequest(unsupported))
            .await
            .err(),
        Some(Error::InvalidIdentity)
    );
    sqlx::query("UPDATE clients SET status='revoked',auth_generation=auth_generation+1")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        f.admit(&target, &proof).await.err(),
        Some(Error::InvalidActor)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_commands")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    f.close().await;
}
#[tokio::test]
async fn stale_clock_floor_or_time_overflow_latches_dangerous_admission_closed() {
    let f = Fixture::new().await;
    sqlx::query("UPDATE product_control SET time_floor_ms=5000")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        f.store
            .checkpoint(f.actor.0, f.epoch, &f.clock, deadline())
            .await
            .err(),
        Some(Error::ClockUnreliable)
    );
    assert_eq!(f.store.control().await.unwrap().1, 5000);
    assert!(f.store.faulted());
    f.close().await;
    assert!(retention(MAX_REQUEST_TIME).is_err());
    let clock = Clock::new();
    clock.sample().unwrap();
    clock.set(999);
    assert!(clock.sample().is_err());
    clock.set(5000);
    assert!(clock.sample().is_err());
}
#[tokio::test]
async fn native_completion_after_actor_revoke_still_records_but_cannot_be_read_by_revoked_actor() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, payload.digest().unwrap(), &cancel)
        .await
        .unwrap();
    let InputAdmission::New { ticket, .. } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    let receipt = service
        .dispatch(&f.actor, permit, &Writer::default(), &cancel)
        .await
        .unwrap();
    sqlx::query("UPDATE clients SET status='revoked',auth_generation=auth_generation+1")
        .execute(&f.pool)
        .await
        .unwrap();
    service
        .report(&receipt, NativeInputReport::Completed, &cancel)
        .await
        .unwrap();
    assert_eq!(
        service
            .command(&f.actor, target.target, receipt.command(), &cancel)
            .await
            .err(),
        Some(Error::InvalidActor)
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM product_commands")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "completed"
    );
    f.close().await;
}

#[derive(Default)]
struct DeferredWriter {
    queue: Mutex<Vec<Arc<AtomicBool>>>,
    entered: tokio::sync::Notify,
    writes: AtomicU64,
}
struct DeferredWriteGuard(Arc<AtomicBool>);
impl Drop for DeferredWriteGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
impl DeferredWriter {
    fn drain(&self) {
        for valid in self.queue.lock().unwrap().drain(..) {
            if valid.load(Ordering::SeqCst) {
                self.writes.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}
impl InputTransport for DeferredWriter {
    async fn write_once(&self, _body: &[u8]) -> Result<(), Error> {
        let valid = Arc::new(AtomicBool::new(true));
        let _owned_attempt = DeferredWriteGuard(valid.clone());
        self.queue.lock().unwrap().push(valid);
        self.entered.notify_one();
        std::future::pending().await
    }
}
#[tokio::test]
async fn cancelled_writer_invalidates_late_queue_and_does_not_hold_sql_or_revocation_gate() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, payload.digest().unwrap(), &cancel)
        .await
        .unwrap();
    let InputAdmission::New { ticket, .. } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    let writer = DeferredWriter::default();
    {
        let dispatch = service.dispatch(&f.actor, permit, &writer, &cancel);
        tokio::pin!(dispatch);
        tokio::select! {r=&mut dispatch=>panic!("write unexpectedly completed: {}",r.is_ok()),()=writer.entered.notified()=>{}}
        // A second SQLite connection gets the write lock while the first writer is blocked.
        let mut sql = f.pool.acquire().await.unwrap();
        tokio::time::timeout(
            Duration::from_millis(200),
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *sql),
        )
        .await
        .unwrap()
        .unwrap();
        cancel.cancel();
        // Drive cancellation into its result-save path, which now waits for the held SQL lock.
        tokio::select! {r=&mut dispatch=>panic!("result save skipped its lock: {}",r.is_ok()),()=tokio::time::sleep(Duration::from_millis(30))=>{}}
        let gate = tokio::time::timeout(Duration::from_millis(100), f.authority.gate.lock())
            .await
            .unwrap();
        drop(gate);
        writer.drain();
        assert_eq!(writer.writes.load(Ordering::SeqCst), 0);
        sqlx::query("COMMIT").execute(&mut *sql).await.unwrap();
        drop(sql);
        assert!(matches!(dispatch.await, Err(Error::Cancelled)));
    }
    assert_eq!(inputs.retained(), (0, 0));
    writer.drain();
    assert_eq!(writer.writes.load(Ordering::SeqCst), 0);
    f.close().await;
}
#[tokio::test]
async fn dropping_ticket_or_permit_cleans_input_and_latches_management_fault() {
    for claimed in [false, true] {
        let f = Fixture::new().await;
        let service = f.service().await;
        let target = f.target(10).await;
        let inputs = MemoryInputs::default();
        let payload = body(&inputs);
        let cancel = Cancellation::default();
        let token = service
            .issue(&f.actor, target.target, payload.digest().unwrap(), &cancel)
            .await
            .unwrap();
        let InputAdmission::New { ticket, .. } = service
            .submit(&f.actor, target.target, &token, payload, &cancel)
            .await
            .unwrap()
        else {
            panic!("new");
        };
        if claimed {
            drop(service.claim(&f.actor, ticket, &cancel).await.unwrap());
        } else {
            drop(ticket);
        }
        assert!(service.faulted());
        assert_eq!(inputs.retained(), (0, 0));
        assert!(
            service
                .issue(&f.actor, target.target, [1; 32], &cancel)
                .await
                .is_err()
        );
        f.store.mark_interrupted(&f.clock).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM product_commands")
                .fetch_one(&f.pool)
                .await
                .unwrap(),
            "unknown"
        );
        f.close().await;
    }
}

#[tokio::test]
async fn mgt_code_001_signed_expiry_retires_a_body_while_its_accepted_ticket_is_retained() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let supervisor = inputs.supervise();
    let mut upload = inputs.reserve(15, 40, deadline()).unwrap();
    upload.append(b"synthetic input").unwrap();
    let payload = upload.finish(&Structure).unwrap();
    let intent = input_intent(f.actor.0.client_id, &target, payload.digest().unwrap()).unwrap();
    let codec = Codec::new();
    let token = codec
        .issue(f.epoch, id(66), &intent, f.clock.sample().unwrap())
        .unwrap();
    let expires = codec
        .verify(&token, f.epoch, &intent, f.clock.sample().unwrap())
        .unwrap()
        .identity()
        .expires_at;
    // Leave admission headroom under parallel test load; retain the ticket across actual expiry.
    f.clock.set(expires - 800);
    let cancel = Cancellation::default();
    let admission = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap();
    assert!(matches!(admission, InputAdmission::New { .. }));
    assert_eq!(inputs.retained(), (55, 1));
    tokio::time::sleep(Duration::from_millis(950)).await;
    assert_eq!(inputs.retained(), (0, 0));
    drop(admission);
    supervisor.shutdown().await;
    f.close().await;
}
#[derive(Clone)]
struct SlowSampleClock {
    clock: Clock,
    calls: Arc<AtomicU64>,
    delay_on: Arc<AtomicU64>,
    delay_ms: u64,
}
impl TimeSource for SlowSampleClock {
    fn sample(&self) -> Result<RequestTime, Error> {
        let sampled = self.clock.sample()?;
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call == self.delay_on.load(Ordering::SeqCst) {
            std::thread::sleep(StdDuration::from_millis(self.delay_ms));
        }
        Ok(sampled)
    }
}
#[tokio::test]
async fn mgt_code_001_anchor_before_fresh_sample_does_not_add_its_latency_to_body_lifetime() {
    let f = Fixture::new().await;
    let time = SlowSampleClock {
        clock: f.clock.clone(),
        calls: Arc::default(),
        delay_on: Arc::new(AtomicU64::new(u64::MAX)),
        delay_ms: 60,
    };
    let service = ProductInputService::start(
        f.store.clone(),
        f.authority.clone(),
        Codec::new(),
        time.clone(),
    )
    .await
    .unwrap();
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let intent = input_intent(f.actor.0.client_id, &target, payload.digest().unwrap()).unwrap();
    let codec = Codec::new();
    let token = codec
        .issue(f.epoch, id(66), &intent, f.clock.sample().unwrap())
        .unwrap();
    let expires = codec
        .verify(&token, f.epoch, &intent, f.clock.sample().unwrap())
        .unwrap()
        .identity()
        .expires_at;
    f.clock.set(expires - 30);
    time.calls.store(0, Ordering::SeqCst);
    time.delay_on.store(2, Ordering::SeqCst);
    assert!(matches!(
        service
            .submit(
                &f.actor,
                target.target,
                &token,
                payload,
                &Cancellation::default()
            )
            .await,
        Err(Error::InputExpired)
    ));
    assert_eq!(inputs.retained(), (0, 0));
    assert_eq!(time.calls.load(Ordering::SeqCst), 2);
    f.close().await;
}
#[tokio::test]
async fn mgt_code_002_shared_fault_during_final_refresh_prevents_the_first_transport_call() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, payload.digest().unwrap(), &cancel)
        .await
        .unwrap();
    let InputAdmission::New { fact, ticket } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    let writer = Writer::default();
    let pause = Arc::new(RefreshPause::default());
    *f.authority.pause_refresh.lock().unwrap() = Some(pause.clone());
    {
        let dispatch = service.dispatch(&f.actor, permit, &writer, &cancel);
        tokio::pin!(dispatch);
        tokio::select! {r=&mut dispatch=>panic!("missing final pause: {}",r.is_ok()),()=pause.entered.notified()=>{}}
        f.store.fault();
        pause.release.notify_one();
        assert!(matches!(dispatch.await, Err(Error::StorageUnavailable)));
    }
    assert_eq!(writer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(inputs.retained(), (0, 0));
    assert!(service.faulted());
    assert_eq!(
        f.store
            .get(f.actor.0, fact.command, deadline())
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Failed
    );
    f.close().await;
}
#[derive(Clone)]
struct FaultSampleClock {
    clock: Clock,
    store: ProductStore,
    calls: Arc<AtomicU64>,
    fault_on: Arc<AtomicU64>,
}
impl TimeSource for FaultSampleClock {
    fn sample(&self) -> Result<RequestTime, Error> {
        let sampled = self.clock.sample()?;
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call == self.fault_on.load(Ordering::SeqCst) {
            self.store.fault();
        }
        Ok(sampled)
    }
}
#[tokio::test]
async fn mgt_code_002_shared_fault_from_final_time_sample_prevents_the_first_transport_call() {
    let f = Fixture::new().await;
    let time = FaultSampleClock {
        clock: f.clock.clone(),
        store: f.store.clone(),
        calls: Arc::default(),
        fault_on: Arc::new(AtomicU64::new(u64::MAX)),
    };
    let service = ProductInputService::start(
        f.store.clone(),
        f.authority.clone(),
        Codec::new(),
        time.clone(),
    )
    .await
    .unwrap();
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, payload.digest().unwrap(), &cancel)
        .await
        .unwrap();
    let InputAdmission::New { ticket, .. } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    let writer = Writer::default();
    time.calls.store(0, Ordering::SeqCst);
    time.fault_on.store(2, Ordering::SeqCst);
    assert!(matches!(
        service.dispatch(&f.actor, permit, &writer, &cancel).await,
        Err(Error::StorageUnavailable)
    ));
    assert_eq!(writer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(inputs.retained(), (0, 0));
    assert!(service.faulted());
    f.close().await;
}
#[tokio::test]
async fn mgt_code_002_fault_after_writer_entry_keeps_the_possibly_sent_result_unknown() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, payload.digest().unwrap(), &cancel)
        .await
        .unwrap();
    let InputAdmission::New { fact, ticket } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    let writer = DeferredWriter::default();
    {
        let dispatch = service.dispatch(&f.actor, permit, &writer, &cancel);
        tokio::pin!(dispatch);
        tokio::select! {r=&mut dispatch=>panic!("missing writer pause: {}",r.is_ok()),()=writer.entered.notified()=>{}}
        f.store.fault();
        cancel.cancel();
        assert!(matches!(dispatch.await, Err(Error::Cancelled)));
    }
    writer.drain();
    assert_eq!(writer.writes.load(Ordering::SeqCst), 0);
    assert_eq!(inputs.retained(), (0, 0));
    assert_eq!(
        f.store
            .get(f.actor.0, fact.command, deadline())
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Unknown
    );
    f.close().await;
}

#[tokio::test]
async fn mgt_code_003_operation_budget_elapsed_keeps_a_valid_signature_failed_not_expired() {
    let f = Fixture::new().await;
    let time = SlowSampleClock {
        clock: f.clock.clone(),
        calls: Arc::default(),
        delay_on: Arc::new(AtomicU64::new(u64::MAX)),
        delay_ms: 10_300,
    };
    let service = ProductInputService::start(
        f.store.clone(),
        f.authority.clone(),
        Codec::new(),
        time.clone(),
    )
    .await
    .unwrap();
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let mut upload = inputs
        .reserve(15, 40, Instant::now() + Duration::from_secs(3600))
        .unwrap();
    upload.append(b"synthetic input").unwrap();
    let payload = upload.finish(&Structure).unwrap();
    let content_digest = payload.digest().unwrap();
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, content_digest, &cancel)
        .await
        .unwrap();
    let InputAdmission::New { fact, ticket } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    let writer = Writer::default();
    time.calls.store(0, Ordering::SeqCst);
    time.delay_on.store(2, Ordering::SeqCst);
    let result = service.dispatch(&f.actor, permit, &writer, &cancel).await;
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(writer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(inputs.retained(), (0, 0));
    Codec::new()
        .verify(
            &token,
            f.epoch,
            &input_intent(f.actor.0.client_id, &target, content_digest).unwrap(),
            f.clock.sample().unwrap(),
        )
        .unwrap();
    assert_eq!(
        f.store
            .get(f.actor.0, fact.command, deadline())
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Failed
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT outcome FROM product_audit_slots WHERE stage='result'"
        )
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        "failed"
    );
    f.close().await;
}
#[tokio::test]
async fn mgt_code_003_object_ttl_elapsed_is_not_product_request_expiry() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let supervisor = inputs.supervise();
    let mut upload = inputs
        .reserve(15, 40, Instant::now() + Duration::from_millis(1500))
        .unwrap();
    upload.append(b"synthetic input").unwrap();
    let payload = upload.finish(&Structure).unwrap();
    let content_digest = payload.digest().unwrap();
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, content_digest, &cancel)
        .await
        .unwrap();
    let InputAdmission::New { fact, ticket } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    let writer = Writer::default();
    let pause = Arc::new(RefreshPause::default());
    *f.authority.pause_refresh.lock().unwrap() = Some(pause.clone());
    {
        let dispatch = service.dispatch(&f.actor, permit, &writer, &cancel);
        tokio::pin!(dispatch);
        tokio::select! {r=&mut dispatch=>panic!("missing refresh pause: {}",r.is_ok()),()=pause.entered.notified()=>{}}
        tokio::time::sleep(Duration::from_millis(1700)).await;
        pause.release.notify_one();
        assert!(matches!(dispatch.await, Err(Error::InputExpired)));
    }
    assert_eq!(writer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(inputs.retained(), (0, 0));
    Codec::new()
        .verify(
            &token,
            f.epoch,
            &input_intent(f.actor.0.client_id, &target, content_digest).unwrap(),
            f.clock.sample().unwrap(),
        )
        .unwrap();
    assert_eq!(
        f.store
            .get(f.actor.0, fact.command, deadline())
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Failed
    );
    supervisor.shutdown().await;
    f.close().await;
}
#[tokio::test]
async fn mgt_code_003_actual_signed_expiry_still_records_expired() {
    let f = Fixture::new().await;
    let service = f.service().await;
    let target = f.target(10).await;
    let inputs = MemoryInputs::default();
    let payload = body(&inputs);
    let content_digest = payload.digest().unwrap();
    let cancel = Cancellation::default();
    let token = service
        .issue(&f.actor, target.target, content_digest, &cancel)
        .await
        .unwrap();
    let expires = Codec::new()
        .verify(
            &token,
            f.epoch,
            &input_intent(f.actor.0.client_id, &target, content_digest).unwrap(),
            f.clock.sample().unwrap(),
        )
        .unwrap()
        .identity()
        .expires_at;
    let InputAdmission::New { fact, ticket } = service
        .submit(&f.actor, target.target, &token, payload, &cancel)
        .await
        .unwrap()
    else {
        panic!("new");
    };
    let permit = service.claim(&f.actor, ticket, &cancel).await.unwrap();
    f.clock.set(expires);
    let writer = Writer::default();
    assert!(matches!(
        service.dispatch(&f.actor, permit, &writer, &cancel).await,
        Err(Error::Expired)
    ));
    assert_eq!(writer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(inputs.retained(), (0, 0));
    assert_eq!(
        f.store
            .get(f.actor.0, fact.command, deadline())
            .await
            .unwrap()
            .status,
        ProductCommandStatus::Expired
    );
    f.close().await;
}
