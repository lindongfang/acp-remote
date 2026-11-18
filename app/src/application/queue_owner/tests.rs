#![allow(
    clippy::significant_drop_tightening,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::*;
use crate::application::request_registry::{
    AuditFailure, AuditMetadata, AuditPort, NodeClock, QueueAction, QueueRejection,
    RequestAuthority, ValidatedBodyPort,
};
use crate::{
    domain::{
        ActorContext,
        native::{HarnessConnectionEpoch, LoadEpoch},
    },
    session_storage::request_registry::test_hooks::{Action, Barrier, Operation, Point},
    storage::{NodeMeta, SCHEMA_VERSION},
};
use sha2::{Digest, Sha256};
use sqlx::{Connection, SqlitePool};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicI64, Ordering},
};
use tokio::{
    sync::Semaphore,
    time::{Duration, Instant},
};

#[path = "automatic_close_tests.rs"]
mod automatic_close_tests;

fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
fn epoch() -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(id(101)),
        connection: HarnessConnectionEpoch(id(102)),
        load: LoadEpoch(id(103)),
    }
}
const TARGET: u8 = 40;
struct Area(PathBuf);
impl Area {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "acpr-reg-dev-{}-{}",
            std::process::id(),
            crate::random::hex(10).unwrap()
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn database(&self) -> PathBuf {
        self.0.join("node.sqlite3")
    }
}
impl Drop for Area {
    fn drop(&mut self) {
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("acpr-reg-dev-")
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Authority(SqlitePool);
impl Authority {
    async fn current(&self, actor: EntityId) -> Result<ActorContext, RegistryError> {
        let row: Option<(String, i64)> =
            sqlx::query_as("SELECT status,auth_generation FROM clients WHERE client_id=?")
                .bind(actor.encode())
                .fetch_optional(&self.0)
                .await?;
        match row {
            Some((status, generation)) if status == "trusted" => Ok(ActorContext {
                client_id: actor,
                auth_generation: generation,
            }),
            _ => Err(RegistryError::InvalidActor),
        }
    }
}
impl RequestAuthority for Authority {
    type Actor = EntityId;
    async fn authorize(
        &self,
        actor: &EntityId,
        _: EntityId,
        _: QueueAction,
    ) -> Result<ActorContext, RegistryError> {
        self.current(*actor).await
    }
    async fn authorize_read(
        &self,
        actor: &EntityId,
        _: EntityId,
    ) -> Result<ActorContext, RegistryError> {
        self.current(*actor).await
    }
    async fn resolve_target(&self, target: EntityId) -> Result<NativeSessionKey, RegistryError> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT source_id,native_id FROM product_targets WHERE target_id=?")
                .bind(target.encode())
                .fetch_optional(&self.0)
                .await?;
        let (source, native) = row.ok_or(RegistryError::InvalidTarget)?;
        NativeSessionKey::from_config(
            Some(EntityId::decode(&source).map_err(|_| RegistryError::CorruptState)?),
            &native,
        )
        .map_err(|_| RegistryError::InvalidTarget)
    }
}
struct Body {
    text: String,
    parts: Vec<u8>,
}
struct Validator;
impl ValidatedBodyPort for Validator {
    type Raw = str;
    type Body = Body;
    fn validate(&self, raw: &str) -> Result<(Body, [u8; 32]), RegistryError> {
        Ok((
            Body {
                text: raw.to_owned(),
                parts: vec![0, 255, 7],
            },
            Sha256::digest(raw.as_bytes()).into(),
        ))
    }
}
struct Clock(Arc<AtomicI64>);
impl NodeClock for Clock {
    fn unix_millis(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}
struct Environment {
    readiness: Mutex<DispatchReadiness>,
    resource: Mutex<Option<ResourceEpoch>>,
}
impl Environment {
    fn new() -> Self {
        Self {
            readiness: Mutex::new(DispatchReadiness::Blocked),
            resource: Mutex::new(Some(epoch())),
        }
    }
    fn set_readiness(&self, readiness: DispatchReadiness) {
        *self.readiness.lock().unwrap() = readiness;
    }
    fn set_resource(&self, resource: Option<ResourceEpoch>) {
        *self.resource.lock().unwrap() = resource;
    }
}
impl QueueEnvironmentPort for Environment {
    fn readiness(&self, _: &NativeSessionKey) -> DispatchReadiness {
        *self.readiness.lock().unwrap()
    }
    fn resource_epoch(&self, _: &NativeSessionKey) -> Option<ResourceEpoch> {
        *self.resource.lock().unwrap()
    }
    fn poll_event(&self, _: &NativeSessionKey) -> Option<QueueOwnerEvent> {
        None
    }
}
struct GoodAudit;
impl AuditPort for GoodAudit {
    fn append<'a>(
        &'a self,
        _: &'a AuditMetadata,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuditFailure>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}
type Service = QueueService<
    Authority,
    Validator,
    crate::session_storage::request_registry::SqliteRequestRegistry,
    Clock,
>;
struct Fixture {
    area: Arc<Area>,
    storage: crate::sqlite_fixture_scope::FixtureStorage,
    storage_scope: crate::sqlite_fixture_scope::SqliteFixtureScope,
    service: Service,
    driver: NativeQueueDriver<Body>,
    environment: Arc<Environment>,
    clock: Arc<AtomicI64>,
}

#[tokio::test]
async fn bridge_read_admission_uses_original_owner_without_an_input() {
    use crate::application::shared_views::budget::ReadReason;
    let f = Fixture::new().await;
    let first = f
        .service
        .prepare_native_read(&id(10), id(TARGET), ReadReason::FirstOpen)
        .await
        .unwrap();
    let second = f
        .service
        .prepare_native_read(&id(11), id(TARGET), ReadReason::FirstOpen)
        .await
        .unwrap();
    assert_eq!(first.target(), id(TARGET));
    assert_eq!(first.native(), second.native());
    assert_eq!(first.owner(), second.owner());
    assert_eq!(f.driver.verify_read_target(&first), Ok(()));
    assert!(
        f.driver
            .accepted_target(id(TARGET))
            .await
            .unwrap()
            .is_none()
    );
    {
        let owners = f.driver.core.owners.lock().unwrap();
        assert_eq!(owners.len(), 1);
        let owner = owners.get(first.native()).unwrap();
        let state = owner.state.lock().unwrap();
        assert!(state.canonical_target.is_none());
        assert_eq!(state.queue.waiting().count(), 0);
        assert!(state.queue.active().is_none());
        assert!(state.pending.is_none());
        assert!(state.origins.is_empty());
        drop(state);
        drop(owners);
    }
    f.close().await;
    assert_eq!(
        first.require_current(),
        Err(crate::domain::native_dispatch::DispatchError::NodeStopping)
    );
}

#[tokio::test]
async fn bridge_read_admission_rejects_foreign_runtime_and_invalid_actor_without_an_owner() {
    use crate::application::shared_views::budget::ReadReason;
    let f = Fixture::new().await;
    assert!(matches!(
        f.service
            .prepare_native_read(&id(99), id(TARGET), ReadReason::FirstOpen)
            .await,
        Err(RegistryError::InvalidActor)
    ));
    assert!(matches!(
        f.service
            .prepare_native_read(&id(10), id(41), ReadReason::FirstOpen)
            .await,
        Err(RegistryError::InvalidTarget)
    ));
    assert!(f.driver.core.owners.lock().unwrap().is_empty());
    let read = f
        .service
        .prepare_native_read(&id(10), id(TARGET), ReadReason::FirstOpen)
        .await
        .unwrap();
    let other = Fixture::new().await;
    assert_eq!(
        other.driver.verify_read_target(&read),
        Err(crate::domain::native_dispatch::DispatchError::InvalidTarget)
    );
    assert!(other.driver.core.owners.lock().unwrap().is_empty());
    other.close().await;
    f.close().await;
}
impl Fixture {
    async fn new() -> Self {
        Self::with_audit(Arc::new(GoodAudit)).await
    }
    async fn with_audit(audit: Arc<dyn AuditPort>) -> Self {
        let area = Arc::new(Area::new());
        let storage_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &area.database(),
            area.clone(),
        );
        let storage = storage_scope.open().await.unwrap();
        storage
            .write_node_meta(&NodeMeta {
                schema_version: SCHEMA_VERSION,
                node_id: "synthetic-reg-node".into(),
                public_key: vec![1, 2, 3],
                key_file_ref: "synthetic-unused.key".into(),
            })
            .await
            .unwrap();
        for n in [10, 11] {
            sqlx::query("INSERT INTO clients VALUES(?,?,?,'trusted',1,0,NULL)")
                .bind(id(n).encode())
                .bind(vec![n])
                .bind(format!("synthetic-{n}"))
                .execute(storage.pool_for_test())
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO product_sources VALUES(?,'synthetic source')")
            .bind(id(1).encode())
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
            .bind(id(TARGET).encode())
            .bind(id(1).encode())
            .bind("synthetic-native")
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        let environment = Arc::new(Environment::new());
        let clock = Arc::new(AtomicI64::new(-1));
        let (service, driver) = storage
            .start_queue_service_with_audit_for_fixture(
                Authority(storage.pool_for_test().clone()),
                Validator,
                Clock(clock.clone()),
                environment.clone(),
                audit,
            )
            .await
            .unwrap();
        assert_eq!(service.startup_status(), StartupStatus::Running);
        Self {
            area,
            storage,
            storage_scope,
            service,
            driver,
            environment,
            clock,
        }
    }
    fn mutation(&self, text: &str) -> ValidatedMutation<Body> {
        ValidatedMutation::Submit(self.service.validate_body(text).unwrap())
    }
    async fn submit(&self, text: &str) -> RegistrationId {
        let mutation = self.mutation(text);
        let fact = self
            .service
            .register(&id(10), id(TARGET), &mutation)
            .await
            .unwrap();
        assert!(matches!(
            self.service
                .consume(&id(10), id(TARGET), fact.id(), mutation)
                .await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled { .. },
                ..
            }
        ));
        fact.id()
    }
    async fn apply(&self, mutation: ValidatedMutation<Body>) -> (RegistrationId, QueueOutcome) {
        let fact = self
            .service
            .register(&id(10), id(TARGET), &mutation)
            .await
            .unwrap();
        let reply = self
            .service
            .consume(&id(10), id(TARGET), fact.id(), mutation)
            .await;
        let ConsumeReply::Resolved {
            fact: RequestFact::Settled {
                outcome: actual, ..
            },
            ..
        } = reply
        else {
            panic!("mutation did not settle");
        };
        (fact.id(), actual)
    }
    async fn view(&self) -> Vec<(EntityId, u64, EntityId, bool, String)> {
        self.service
            .inspect_queue(&id(11), id(TARGET), |view| {
                let queue = match view {
                    QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } => queue,
                };
                queue
                    .waiting()
                    .map(|e| {
                        assert_eq!(e.input().body.body().parts, vec![0, 255, 7]);
                        (
                            e.id(),
                            e.revision(),
                            e.editor(),
                            e.suspended(),
                            e.input().body.body().text.clone(),
                        )
                    })
                    .collect()
            })
            .await
            .unwrap()
    }
    async fn close(self) {
        let report = self
            .service
            .shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(3),
            ))
            .await;
        assert!(report.joined, "{report:?}");
        self.storage.close().await;
        self.storage_scope.stop_and_observe().await;
        assert!(self.area.database().exists());
    }
}

#[tokio::test]
async fn registry_node_clock_adjustment_does_not_expire_or_reorder_queue() {
    let f = Fixture::new().await;
    f.clock.store(1_800_000_000_000, Ordering::SeqCst);
    let first = f.submit("clock-first").await;

    f.clock.store(1_800_086_400_000, Ordering::SeqCst);
    let second = f.submit("clock-second").await;

    f.clock.store(1_700_000_000_000, Ordering::SeqCst);
    let third = f.submit("clock-third-after-backward-adjustment").await;

    assert_eq!(
        f.view()
            .await
            .into_iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        vec![first.entity(), second.entity(), third.entity()],
        "wall-clock movement must not expire or reorder accepted queue items"
    );

    let request = f.mutation("clock-adjustment-does-not-lock-admission");
    let registered = f
        .service
        .register(&id(11), id(TARGET), &request)
        .await
        .unwrap();
    assert!(matches!(
        f.service
            .consume(&id(11), id(TARGET), registered.id(), request)
            .await,
        ConsumeReply::Resolved {
            fact: RequestFact::Settled {
                outcome: QueueOutcome::Rejected(QueueRejection::Full),
                ..
            },
            ..
        }
    ));
    assert_eq!(
        f.view()
            .await
            .into_iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        vec![first.entity(), second.entity(), third.entity()]
    );
    f.close().await;
}

#[tokio::test]
async fn registry_selected_priority_edit_pauses_every_item_and_never_falls_back() {
    let f = Fixture::new().await;
    f.driver.enable_native_extension();
    let x = f.submit("x").await;
    let y = f.submit("y").await;
    let z = f.submit("z").await;
    let prioritize = ValidatedMutation::Prioritize {
        item: y.entity(),
        revision: 1,
    };
    let (_, outcome) = f.apply(prioritize).await;
    assert_eq!(
        outcome,
        QueueOutcome::Prioritized {
            item: y.entity(),
            revision: 1,
        }
    );
    let owner = f.driver.core.owner(id(TARGET)).await.unwrap();
    assert_eq!(
        lock(&owner.state)
            .unwrap()
            .selected_priority
            .map(|(_, item, revision)| (item, revision)),
        Some((y.entity(), 1))
    );

    let edit = ValidatedMutation::Edit {
        item: y.entity(),
        revision: 1,
        body: f.service.validate_body("y-v2").unwrap(),
    };
    let (_, outcome) = f.apply(edit).await;
    assert_eq!(
        outcome,
        QueueOutcome::Edited {
            item: y.entity(),
            revision: 2,
        }
    );

    {
        let mut state = lock(&owner.state).unwrap();
        assert_eq!(state.selected_priority, None);
        assert!(state.priority_handoff.is_none());
        assert_eq!(
            state.queue.priority_failure(),
            Some(PriorityFailure::SelectionChanged)
        );
        assert_eq!(state.queue.last_stop_reason(), None);
        assert_eq!(
            state
                .queue
                .waiting()
                .map(|entry| (entry.id(), entry.revision(), entry.suspended()))
                .collect::<Vec<_>>(),
            vec![
                (x.entity(), 1, true),
                (y.entity(), 2, true),
                (z.entity(), 1, true),
            ]
        );
        assert!(
            state
                .queue
                .prepare(DispatchReadiness::Ready(epoch()))
                .unwrap()
                .is_none()
        );
    }
    let dispatches: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_native_dispatches")
        .fetch_one(f.storage.pool_for_test())
        .await
        .unwrap();
    assert_eq!(dispatches, 0);
    f.close().await;
}

#[tokio::test]
async fn registry_selected_priority_cancel_pauses_remaining_items_and_never_falls_back() {
    let f = Fixture::new().await;
    f.driver.enable_native_extension();
    let x = f.submit("x").await;
    let y = f.submit("y").await;
    let z = f.submit("z").await;
    let (_, outcome) = f
        .apply(ValidatedMutation::Prioritize {
            item: y.entity(),
            revision: 1,
        })
        .await;
    assert!(matches!(outcome, QueueOutcome::Prioritized { .. }));

    let (_, outcome) = f
        .apply(ValidatedMutation::Cancel {
            item: y.entity(),
            revision: 1,
        })
        .await;
    assert_eq!(
        outcome,
        QueueOutcome::Cancelled {
            item: y.entity(),
            revision: 1,
        }
    );
    let owner = f.driver.core.owner(id(TARGET)).await.unwrap();
    {
        let mut state = lock(&owner.state).unwrap();
        assert_eq!(state.selected_priority, None);
        assert!(state.priority_handoff.is_none());
        assert_eq!(
            state.queue.priority_failure(),
            Some(PriorityFailure::SelectionChanged)
        );
        assert_eq!(
            state
                .queue
                .waiting()
                .map(|entry| (entry.id(), entry.suspended()))
                .collect::<Vec<_>>(),
            vec![(x.entity(), true), (z.entity(), true)]
        );
        assert!(
            state
                .queue
                .prepare(DispatchReadiness::Ready(epoch()))
                .unwrap()
                .is_none()
        );
    }
    let dispatches: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_native_dispatches")
        .fetch_one(f.storage.pool_for_test())
        .await
        .unwrap();
    assert_eq!(dispatches, 0);
    f.close().await;
}

#[tokio::test]
async fn registry_selected_priority_resource_change_is_stale_and_pauses_without_fallback() {
    let f = Fixture::new().await;
    f.driver.enable_native_extension();
    let x = f.submit("x").await;
    let y = f.submit("y").await;
    let z = f.submit("z").await;
    f.apply(ValidatedMutation::Prioritize {
        item: y.entity(),
        revision: 1,
    })
    .await;
    let handoff = f
        .driver
        .take_priority_handoff(id(TARGET))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(handoff.resource(), Some(epoch()));
    let changed = ResourceEpoch {
        load: LoadEpoch(id(104)),
        ..epoch()
    };
    f.environment.set_resource(Some(changed));
    assert_eq!(
        f.driver
            .priority_current_for_test(id(TARGET), &handoff)
            .await,
        Err(crate::domain::native_dispatch::DispatchError::StaleSelection)
    );
    let native = f
        .driver
        .core
        .owners
        .lock()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    f.driver
        .fail_priority_for_native(&native, PriorityFailure::SelectionChanged)
        .await
        .unwrap();

    let owner = f.driver.core.owner(id(TARGET)).await.unwrap();
    {
        let state = lock(&owner.state).unwrap();
        assert_eq!(
            state.queue.priority_failure(),
            Some(PriorityFailure::SelectionChanged)
        );
        assert_eq!(
            state
                .queue
                .waiting()
                .map(|entry| (entry.id(), entry.suspended()))
                .collect::<Vec<_>>(),
            vec![(x.entity(), true), (y.entity(), true), (z.entity(), true),]
        );
    }
    let dispatches: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_native_dispatches")
        .fetch_one(f.storage.pool_for_test())
        .await
        .unwrap();
    assert_eq!(dispatches, 0);
    f.close().await;
}

#[tokio::test]
async fn registry_selected_priority_interrupt_failure_pauses_without_fallback() {
    let f = Fixture::new().await;
    f.driver.enable_native_extension();
    let x = f.submit("x").await;
    let y = f.submit("y").await;
    let z = f.submit("z").await;
    f.apply(ValidatedMutation::Prioritize {
        item: y.entity(),
        revision: 1,
    })
    .await;
    let native = f
        .driver
        .core
        .owners
        .lock()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    f.driver
        .fail_priority_for_native(&native, PriorityFailure::InterruptUnconfirmed)
        .await
        .unwrap();

    let owner = f.driver.core.owner(id(TARGET)).await.unwrap();
    {
        let mut state = lock(&owner.state).unwrap();
        assert_eq!(
            state.queue.priority_failure(),
            Some(PriorityFailure::InterruptUnconfirmed)
        );
        assert_eq!(
            state
                .queue
                .waiting()
                .map(|entry| (entry.id(), entry.suspended()))
                .collect::<Vec<_>>(),
            vec![(x.entity(), true), (y.entity(), true), (z.entity(), true),]
        );
        assert!(
            state
                .queue
                .prepare(DispatchReadiness::Ready(epoch()))
                .unwrap()
                .is_none()
        );
    }
    let dispatches: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_native_dispatches")
        .fetch_one(f.storage.pool_for_test())
        .await
        .unwrap();
    assert_eq!(dispatches, 0);
    f.close().await;
}

#[tokio::test]
async fn registry_live_terminal_pauses_before_external_marker_without_creating_run() {
    let f = Fixture::new().await;
    f.driver.enable_native_extension();
    let x = f.submit("x").await;
    let y = f.submit("y").await;
    let z = f.submit("z").await;
    let resource = ResourceEpoch {
        runtime: f.driver.core.runtime,
        ..epoch()
    };
    f.environment.set_resource(Some(resource));
    let owner = f.driver.core.owner(id(TARGET)).await.unwrap();
    let queue = super::native::observation_link(f.driver.core.clone(), owner.clone());

    let external = queue
        .apply(resource, &mut |_| {
            Ok(NativeQueuePolicy::EndExternal {
                disposition: QueueDisposition::Suspend,
                reason: crate::domain::native_dispatch::KnownStopReason::MaxTokens,
            })
        })
        .unwrap();

    assert!(external.is_none());
    {
        let state = lock(&owner.state).unwrap();
        assert!(state.external.is_none());
        assert!(state.active.is_none());
        assert_eq!(
            state.queue.last_stop_reason(),
            Some(crate::domain::native_dispatch::KnownStopReason::MaxTokens)
        );
        assert!(state.queue.failure_suspended());
        assert_eq!(
            state
                .queue
                .waiting()
                .map(|entry| (entry.id(), entry.suspended()))
                .collect::<Vec<_>>(),
            vec![(x.entity(), true), (y.entity(), true), (z.entity(), true),]
        );
    }
    let dispatches: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_native_dispatches")
        .fetch_one(f.storage.pool_for_test())
        .await
        .unwrap();
    assert_eq!(
        dispatches, 0,
        "a session-level terminal without a Run association must not invent one"
    );
    f.close().await;
}

#[tokio::test]
async fn registry_real_owner_deduplicates_and_preserves_cross_actor_edit_binding() {
    let f = Fixture::new().await;
    let x = f.submit("original-canary").await;
    assert!(matches!(
        f.service
            .consume(&id(10), id(TARGET), x, f.mutation("original-canary"))
            .await,
        ConsumeReply::Resolved {
            returned: Some(_),
            ..
        }
    ));
    let edit = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 1,
        body: f.service.validate_body("edited-canary").unwrap(),
    };
    let operation = f
        .service
        .register(&id(11), id(TARGET), &edit)
        .await
        .unwrap();
    assert!(matches!(
        f.service
            .consume(&id(11), id(TARGET), operation.id(), edit)
            .await,
        ConsumeReply::Resolved {
            fact: RequestFact::Settled {
                outcome: QueueOutcome::Edited { revision: 2, .. },
                ..
            },
            ..
        }
    ));
    let again = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 1,
        body: f.service.validate_body("edited-canary").unwrap(),
    };
    f.service
        .consume(&id(11), id(TARGET), operation.id(), again)
        .await;
    assert_eq!(
        f.view().await,
        vec![(x.entity(), 2, id(11), false, "edited-canary".into())]
    );
    assert!(matches!(
        f.service
            .consume(&id(10), id(TARGET), x, f.mutation("edited-canary"))
            .await,
        ConsumeReply::Refused {
            error: RegistryError::Conflict,
            ..
        }
    ));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_runs")
        .fetch_one(f.storage.pool_for_test())
        .await
        .unwrap();
    assert_eq!(count, 0);
    f.close().await;
}

#[tokio::test]
async fn registry_begin_commit_pause_does_not_block_external_failure_before_new_input() {
    let f = Fixture::new().await;
    f.submit("x").await;
    f.submit("y").await;
    let work = f
        .driver
        .open_external_work(id(TARGET), epoch())
        .await
        .unwrap();
    let d = f.mutation("new-d");
    let registration = f.service.register(&id(10), id(TARGET), &d).await.unwrap();
    let barrier = Barrier::new();
    f.storage
        .request_registry()
        .faults_for_test()
        .arm(
            Point::AfterCommit(Operation::Begin),
            Action::Pause(barrier.clone()),
        )
        .unwrap();
    let service = f.service.clone();
    let request = registration.id();
    let submit =
        tokio::spawn(async move { service.consume(&id(10), id(TARGET), request, d).await });
    barrier.arrived().await;
    let phase: String =
        sqlx::query_scalar("SELECT phase FROM node_request_operations WHERE request_id=?")
            .bind(request.encode())
            .fetch_one(f.storage.pool_for_test())
            .await
            .unwrap();
    assert_eq!(phase, "applying");
    f.driver
        .observe(
            id(TARGET),
            QueueOwnerEvent::ExternalForegroundFailure { work: work.clone() },
        )
        .await
        .unwrap();
    barrier.release();
    assert!(matches!(
        submit.await.unwrap(),
        ConsumeReply::Resolved { .. }
    ));
    let view = f.view().await;
    assert_eq!(
        view.iter().map(|e| e.3).collect::<Vec<_>>(),
        [true, true, false]
    );
    assert!(matches!(
        f.driver
            .observe(
                id(TARGET),
                QueueOwnerEvent::ExternalForegroundFailure { work }
            )
            .await,
        Err(DriverError::Registry(RegistryError::StaleObservation))
    ));
    assert!(!f.view().await[2].3);
    f.close().await;
}

#[tokio::test]
async fn registry_settle_failure_retains_once_applied_revision_and_internal_recovery() {
    let f = Fixture::new().await;
    let x = f.submit("x").await;
    let edit = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 1,
        body: f.service.validate_body("x-v2").unwrap(),
    };
    let r = f
        .service
        .register(&id(10), id(TARGET), &edit)
        .await
        .unwrap();
    f.storage
        .request_registry()
        .faults_for_test()
        .arm(Point::SettleUpdate, Action::Fail)
        .unwrap();
    assert!(matches!(
        f.service.consume(&id(10), id(TARGET), r.id(), edit).await,
        ConsumeReply::Pending { .. }
    ));
    assert!(
        f.service
            .inspect_queue(&id(11), id(TARGET), |v| matches!(
                v,
                QueueRead::Processing { applied: true, .. }
            ))
            .await
            .unwrap()
    );
    assert_eq!(f.view().await[0].1, 2);
    sqlx::query("UPDATE clients SET status='revoked',auth_generation=2 WHERE client_id=?")
        .bind(id(10).encode())
        .execute(f.storage.pool_for_test())
        .await
        .unwrap();
    assert!(matches!(
        f.service.reconcile(id(TARGET), r.id()).await.unwrap(),
        RequestFact::Settled {
            outcome: QueueOutcome::Edited { revision: 2, .. },
            ..
        }
    ));
    assert_eq!(f.view().await[0].1, 2);
    assert_eq!(
        f.service.query_request(&id(10), id(TARGET), r.id()).await,
        Err(RegistryError::InvalidActor)
    );
    f.close().await;
}

#[tokio::test]
async fn registry_after_commit_unknown_restores_unapplied_lease_only_once() {
    let f = Fixture::new().await;
    let mutation = f.mutation("once");
    let r = f
        .service
        .register(&id(10), id(TARGET), &mutation)
        .await
        .unwrap();
    f.storage
        .request_registry()
        .faults_for_test()
        .arm(Point::AfterCommit(Operation::Begin), Action::Unconfirmed)
        .unwrap();
    assert!(matches!(
        f.service
            .consume(&id(10), id(TARGET), r.id(), mutation)
            .await,
        ConsumeReply::Pending { .. }
    ));
    assert!(f.view().await.is_empty());
    assert!(matches!(
        f.service.reconcile(id(TARGET), r.id()).await.unwrap(),
        RequestFact::Settled { .. }
    ));
    assert_eq!(
        f.service.reconcile(id(TARGET), r.id()).await,
        Err(RegistryError::NoOwnedOperation)
    );
    assert_eq!(f.view().await.len(), 1);
    f.close().await;
}

#[tokio::test]
async fn registry_pending_settle_failure_happens_after_input_and_does_not_undo_suspend() {
    let f = Fixture::new().await;
    f.submit("x").await;
    f.submit("y").await;
    let work = f
        .driver
        .open_external_work(id(TARGET), epoch())
        .await
        .unwrap();
    let mutation = f.mutation("d");
    let r = f
        .service
        .register(&id(10), id(TARGET), &mutation)
        .await
        .unwrap();
    let barrier = Barrier::new();
    f.storage
        .request_registry()
        .faults_for_test()
        .arm(Point::SettleUpdate, Action::Pause(barrier.clone()))
        .unwrap();
    let service = f.service.clone();
    let request = r.id();
    let task = tokio::spawn(async move {
        service
            .consume(&id(10), id(TARGET), request, mutation)
            .await
    });
    barrier.arrived().await;
    f.driver
        .observe(
            id(TARGET),
            QueueOwnerEvent::ExternalForegroundFailure { work },
        )
        .await
        .unwrap();
    assert!(f.view().await.iter().all(|e| e.3));
    barrier.release();
    task.await.unwrap();
    assert!(f.view().await.iter().all(|e| e.3));
    f.close().await;
}

#[tokio::test]
async fn registry_failure_full_waiting_keeps_direct_active_and_stable_rejected_retry() {
    let f = Fixture::new().await;
    let x = f.submit("x").await;
    f.submit("y").await;
    f.submit("z").await;
    let work = f
        .driver
        .open_external_work(id(TARGET), epoch())
        .await
        .unwrap();
    f.driver
        .observe(
            id(TARGET),
            QueueOwnerEvent::ExternalForegroundFailure { work },
        )
        .await
        .unwrap();
    f.environment
        .set_readiness(DispatchReadiness::Ready(epoch()));
    let direct = f.submit("d").await;
    let active = f.driver.active_key(id(TARGET)).await.unwrap().unwrap();
    assert_eq!(active.item(), direct.entity());
    let excess = f.mutation("e");
    let registration = f
        .service
        .register(&id(10), id(TARGET), &excess)
        .await
        .unwrap();
    assert!(matches!(
        f.service
            .consume(&id(10), id(TARGET), registration.id(), excess)
            .await,
        ConsumeReply::Resolved {
            fact: RequestFact::Settled {
                outcome: QueueOutcome::Rejected(QueueRejection::Full),
                ..
            },
            returned: Some(_)
        }
    ));
    f.driver
        .observe(
            id(TARGET),
            QueueOwnerEvent::Settle {
                active,
                disposition: QueueDisposition::Advance,
            },
        )
        .await
        .unwrap();
    let cancel = ValidatedMutation::Cancel {
        item: x.entity(),
        revision: 1,
    };
    let cancellation = f
        .service
        .register(&id(11), id(TARGET), &cancel)
        .await
        .unwrap();
    f.service
        .consume(&id(11), id(TARGET), cancellation.id(), cancel)
        .await;
    assert!(matches!(
        f.service
            .consume(&id(10), id(TARGET), registration.id(), f.mutation("e"))
            .await,
        ConsumeReply::Resolved {
            fact: RequestFact::Settled {
                outcome: QueueOutcome::Rejected(QueueRejection::Full),
                ..
            },
            ..
        }
    ));
    assert_eq!(f.view().await.len(), 2);
    f.close().await;
}

struct HangingAudit {
    entered: Semaphore,
}
impl AuditPort for HangingAudit {
    fn append<'a>(
        &'a self,
        _: &'a AuditMetadata,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuditFailure>> + Send + 'a>> {
        Box::pin(async move {
            self.entered.add_permits(1);
            std::future::pending().await
        })
    }
}
#[tokio::test]
async fn registry_audit_wait_does_not_hold_pending_and_shutdown_joins_its_tasks() {
    let audit = Arc::new(HangingAudit {
        entered: Semaphore::new(0),
    });
    let f = Fixture::with_audit(audit.clone()).await;
    f.submit("x").await;
    audit.entered.acquire().await.unwrap().forget();
    f.submit("y").await;
    audit.entered.acquire().await.unwrap().forget();
    assert_eq!(f.view().await.len(), 2);
    let report = f
        .service
        .shutdown(RegistryDeadline::new(
            Instant::now() + Duration::from_secs(3),
        ))
        .await;
    assert!(report.joined);
    assert_eq!(report.audit_cancelled, 2);
    assert_eq!(f.service.take_audit_warnings().len(), 2);
    f.storage.close().await;
    f.storage_scope.stop_and_observe().await;
}

#[tokio::test]
async fn registry_shutdown_deadline_keeps_claim_until_cancelled_node_task_is_joined() {
    let f = Fixture::new().await;
    let mutation = f.mutation("pending");
    let r = f
        .service
        .register(&id(10), id(TARGET), &mutation)
        .await
        .unwrap();
    let barrier = Barrier::new();
    f.storage
        .request_registry()
        .faults_for_test()
        .arm(
            Point::AfterCommit(Operation::Begin),
            Action::Pause(barrier.clone()),
        )
        .unwrap();
    let service = f.service.clone();
    let request = r.id();
    let task = tokio::spawn(async move {
        service
            .consume(&id(10), id(TARGET), request, mutation)
            .await
    });
    barrier.arrived().await;
    let report = f
        .service
        .shutdown(RegistryDeadline::new(Instant::now()))
        .await;
    assert!(!report.joined);
    assert_eq!(report.unresolved, vec![request]);
    assert!(matches!(
        f.storage
            .start_queue_service_with_audit_for_fixture(
                Authority(f.storage.pool_for_test().clone()),
                Validator,
                Clock(Arc::new(AtomicI64::new(0))),
                f.environment.clone(),
                Arc::new(GoodAudit)
            )
            .await,
        Err(RegistryError::RuntimeAlreadyStarted)
    ));
    barrier.release();
    let _ = task.await;
    let report = f
        .service
        .shutdown(RegistryDeadline::new(
            Instant::now() + Duration::from_secs(3),
        ))
        .await;
    assert!(report.joined);
    let (service, _) = f
        .storage
        .start_queue_service_with_audit_for_fixture(
            Authority(f.storage.pool_for_test().clone()),
            Validator,
            Clock(Arc::new(AtomicI64::new(0))),
            f.environment.clone(),
            Arc::new(GoodAudit),
        )
        .await
        .unwrap();
    assert!(matches!(
        service
            .query_request(&id(11), id(TARGET), request)
            .await
            .unwrap(),
        RequestFact::Interrupted { .. }
    ));
    assert_eq!(
        service
            .inspect_queue(&id(11), id(TARGET), |v| match v {
                QueueRead::Stable { queue } | QueueRead::Processing { queue, .. } =>
                    queue.waiting().len(),
            })
            .await
            .unwrap(),
        0
    );
    assert!(
        service
            .shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(3)
            ))
            .await
            .joined
    );
    f.storage.close().await;
    f.storage_scope.stop_and_observe().await;
}

#[tokio::test]
async fn registry_random_unavailable_and_collision_preserve_existing_identity() {
    use crate::application::request_registry::random_hooks as rng;
    let f = Fixture::new().await;
    let original = f.submit("existing").await;
    let randoms = f.storage.queue_random_faults_for_test();
    randoms
        .arm(
            rng::Point::Registration,
            rng::Action::Value(original.entity()),
        )
        .unwrap();
    let mutation = f.mutation("new");
    let fresh = f
        .service
        .register(&id(10), id(TARGET), &mutation)
        .await
        .unwrap();
    assert_ne!(fresh.id(), original);
    randoms
        .arm(rng::Point::Registration, rng::Action::Unavailable)
        .unwrap();
    assert_eq!(
        f.service.register(&id(10), id(TARGET), &mutation).await,
        Err(RegistryError::RandomUnavailable)
    );
    assert_eq!(f.view().await.len(), 1);
    assert!(
        f.service
            .shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(3)
            ))
            .await
            .joined
    );
    randoms
        .arm(rng::Point::Runtime, rng::Action::Unavailable)
        .unwrap();
    assert!(matches!(
        f.storage
            .start_queue_service_with_audit_for_fixture(
                Authority(f.storage.pool_for_test().clone()),
                Validator,
                Clock(Arc::new(AtomicI64::new(0))),
                f.environment.clone(),
                Arc::new(GoodAudit)
            )
            .await,
        Err(RegistryError::RandomUnavailable)
    ));
    let (service, _) = f
        .storage
        .start_queue_service_with_audit_for_fixture(
            Authority(f.storage.pool_for_test().clone()),
            Validator,
            Clock(Arc::new(AtomicI64::new(0))),
            f.environment.clone(),
            Arc::new(GoodAudit),
        )
        .await
        .unwrap();
    assert!(
        service
            .shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(3)
            ))
            .await
            .joined
    );
    f.storage.close().await;
    f.storage_scope.stop_and_observe().await;
}

#[tokio::test]
async fn registry_external_work_maximum_serial_never_rewraps_or_reuses_a_key() {
    let f = Fixture::new().await;
    let owner = f.driver.core.owner(id(TARGET)).await.unwrap();
    lock(&owner.state).unwrap().external_serial = u64::MAX;
    assert!(matches!(
        f.driver.open_external_work(id(TARGET), epoch()).await,
        Err(DriverError::SequenceExhausted)
    ));
    assert_eq!(lock(&owner.state).unwrap().external_serial, u64::MAX);
    assert!(lock(&owner.state).unwrap().external.is_none());
    f.close().await;
}

#[tokio::test]
async fn registry_storage_closes_startup_when_factory_waiter_drops_before_or_after_commit() {
    struct TrackedClock {
        token: Arc<()>,
        at: i64,
    }
    impl NodeClock for TrackedClock {
        fn unix_millis(&self) -> i64 {
            let _ = &self.token;
            self.at
        }
    }
    for point in [
        Point::BeforeCommit(Operation::Startup),
        Point::AfterCommit(Operation::Startup),
    ] {
        let area = Arc::new(Area::new());
        let storage_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &area.database(),
            area.clone(),
        );
        let storage = Arc::new(storage_scope.open().await.unwrap());
        storage
            .write_node_meta(&NodeMeta {
                schema_version: SCHEMA_VERSION,
                node_id: "synthetic-startup-node".into(),
                public_key: vec![5, 6],
                key_file_ref: "synthetic-unused.key".into(),
            })
            .await
            .unwrap();
        let barrier = Barrier::new();
        storage
            .request_registry()
            .faults_for_test()
            .arm(point, Action::Pause(barrier.clone()))
            .unwrap();
        let environment = Arc::new(Environment::new());
        let owned_storage = storage.clone();
        let owned_environment = environment.clone();
        let retained = Arc::new(());
        let weak = Arc::downgrade(&retained);
        let waiter = tokio::spawn(async move {
            owned_storage
                .start_queue_service_with_audit_for_fixture(
                    Authority(owned_storage.pool_for_test().clone()),
                    Validator,
                    TrackedClock {
                        token: retained,
                        at: 0,
                    },
                    owned_environment,
                    Arc::new(GoodAudit),
                )
                .await
        });
        barrier.arrived().await;
        waiter.abort();
        assert!(waiter.await.is_err());
        assert!(matches!(
            storage
                .start_queue_service_with_audit_for_fixture(
                    Authority(storage.pool_for_test().clone()),
                    Validator,
                    Clock(Arc::new(AtomicI64::new(0))),
                    environment.clone(),
                    Arc::new(GoodAudit)
                )
                .await,
            Err(RegistryError::RuntimeAlreadyStarted)
        ));
        tokio::time::timeout(Duration::from_secs(3), storage.close())
            .await
            .unwrap();
        assert!(
            weak.upgrade().is_none(),
            "Storage must release its joined startup supervisor"
        );
        barrier.release();
        storage_scope.stop_and_observe().await;
        let reopen_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &area.database(),
            area.clone(),
        );
        let reopened = reopen_scope.open().await.unwrap();
        let (service, _) = reopened
            .start_queue_service_with_audit_for_fixture(
                Authority(reopened.pool_for_test().clone()),
                Validator,
                Clock(Arc::new(AtomicI64::new(0))),
                environment,
                Arc::new(GoodAudit),
            )
            .await
            .unwrap();
        assert_eq!(service.startup_status(), StartupStatus::Running);
        reopened.close().await;
        reopen_scope.stop_and_observe().await;
    }
}

async fn join_is_waiting(fixture: &Fixture) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let waiting = lock(&fixture.driver.core.tasks)
                .unwrap()
                .iter()
                .any(|slot| slot.handle.try_lock().is_err());
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn registry_concurrent_shutdown_cannot_report_joined_while_another_closer_owns_join() {
    let fixture = Fixture::new().await;
    let mutation = fixture.mutation("concurrent-close");
    let registration = fixture
        .service
        .register(&id(10), id(TARGET), &mutation)
        .await
        .unwrap();
    let barrier = Barrier::new();
    fixture
        .storage
        .request_registry()
        .faults_for_test()
        .arm(
            Point::AfterCommit(Operation::Begin),
            Action::Pause(barrier.clone()),
        )
        .unwrap();
    let service = fixture.service.clone();
    let request = registration.id();
    let submit = tokio::spawn(async move {
        service
            .consume(&id(10), id(TARGET), request, mutation)
            .await
    });
    barrier.arrived().await;
    let service = fixture.service.clone();
    let first = tokio::spawn(async move {
        service
            .shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(3),
            ))
            .await
    });
    join_is_waiting(&fixture).await;
    let mut second = Box::pin(fixture.service.shutdown(RegistryDeadline::new(
        Instant::now() + Duration::from_secs(3),
    )));
    let pending =
        std::future::poll_fn(|cx| std::task::Poll::Ready(second.as_mut().poll(cx).is_pending()))
            .await;
    assert!(
        pending,
        "a second closer must not infer joined from an empty local task vector"
    );
    assert!(matches!(
        fixture
            .storage
            .start_queue_service_with_audit_for_fixture(
                Authority(fixture.storage.pool_for_test().clone()),
                Validator,
                Clock(Arc::new(AtomicI64::new(0))),
                fixture.environment.clone(),
                Arc::new(GoodAudit)
            )
            .await,
        Err(RegistryError::RuntimeAlreadyStarted)
    ));
    barrier.release();
    assert!(first.await.unwrap().joined);
    assert!(second.await.joined);
    let _ = submit.await.unwrap();
    fixture.close().await;
}

#[tokio::test]
async fn registry_cancelled_shutdown_keeps_pending_join_reachable_from_storage_close() {
    for operation in [Operation::Begin, Operation::Settle] {
        let fixture = Fixture::new().await;
        let mutation = fixture.mutation("cancel-close");
        let registration = fixture
            .service
            .register(&id(10), id(TARGET), &mutation)
            .await
            .unwrap();
        let barrier = Barrier::new();
        fixture
            .storage
            .request_registry()
            .faults_for_test()
            .arm(
                Point::AfterCommit(operation),
                Action::Pause(barrier.clone()),
            )
            .unwrap();
        let service = fixture.service.clone();
        let request = registration.id();
        let submit = tokio::spawn(async move {
            service
                .consume(&id(10), id(TARGET), request, mutation)
                .await
        });
        barrier.arrived().await;
        let mut closing = Box::pin(fixture.service.shutdown(RegistryDeadline::new(
            Instant::now() + Duration::from_secs(3),
        )));
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(
                closing.as_mut().poll(cx).is_pending()
            ))
            .await
        );
        drop(closing);
        assert!(
            lock(&fixture.driver.core.tasks)
                .unwrap()
                .iter()
                .any(|slot| slot
                    .handle
                    .try_lock()
                    .is_ok_and(|handle| handle.as_ref().is_some_and(|task| !task.is_finished())))
        );
        let mut storage_close = Box::pin(fixture.storage.close());
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(
                storage_close.as_mut().poll(cx).is_pending()
            ))
            .await
        );
        barrier.release();
        tokio::time::timeout(Duration::from_secs(3), storage_close)
            .await
            .unwrap();
        assert_eq!(fixture.service.startup_status(), StartupStatus::Stopped);
        let _ = submit.await.unwrap();
        assert!(lock(&fixture.driver.core.tasks).unwrap().is_empty());
    }
}

#[tokio::test]
async fn registry_finished_task_slots_are_joined_and_reclaimed_during_service_lifetime() {
    let fixture = Fixture::new().await;
    for serial in 0..32 {
        let request = fixture.submit(&format!("item-{serial}")).await;
        let cancel = ValidatedMutation::Cancel {
            item: request.entity(),
            revision: 1,
        };
        let registration = fixture
            .service
            .register(&id(11), id(TARGET), &cancel)
            .await
            .unwrap();
        fixture
            .service
            .consume(&id(11), id(TARGET), registration.id(), cancel)
            .await;
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            assert_eq!(fixture.service.task_failures(), 0);
            if lock(&fixture.driver.core.tasks).unwrap().is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    fixture.close().await;
}

#[tokio::test]
async fn registry_old_stopped_service_never_releases_a_new_runtime_claim() {
    let fixture = Fixture::new().await;
    assert!(
        fixture
            .service
            .shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(3)
            ))
            .await
            .joined
    );
    let (next, _) = fixture
        .storage
        .start_queue_service_with_audit_for_fixture(
            Authority(fixture.storage.pool_for_test().clone()),
            Validator,
            Clock(Arc::new(AtomicI64::new(0))),
            fixture.environment.clone(),
            Arc::new(GoodAudit),
        )
        .await
        .unwrap();
    let mut closers = Vec::new();
    for _ in 0..8 {
        let old = fixture.service.clone();
        closers.push(tokio::spawn(async move {
            old.cancel();
            old.shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(3),
            ))
            .await
        }));
    }
    for closer in closers {
        assert!(closer.await.unwrap().joined);
    }
    assert_eq!(fixture.service.startup_status(), StartupStatus::Stopped);
    assert_eq!(next.startup_status(), StartupStatus::Running);
    assert!(matches!(
        fixture
            .storage
            .start_queue_service_with_audit_for_fixture(
                Authority(fixture.storage.pool_for_test().clone()),
                Validator,
                Clock(Arc::new(AtomicI64::new(0))),
                fixture.environment.clone(),
                Arc::new(GoodAudit),
            )
            .await,
        Err(RegistryError::RuntimeAlreadyStarted)
    ));
    fixture.storage.close().await;
    fixture.storage_scope.stop_and_observe().await;
}

#[cfg(windows)]
fn assert_registry_file_exclusive(fixture: &Fixture) {
    use std::os::windows::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(fixture.area.database())
        .unwrap();
    drop(file);
}

#[tokio::test]
async fn registry_connection_close_survives_cancelled_operation_and_storage_waiters() {
    use crate::session_storage::request_registry::connection_close_test_hooks::{
        CloseBarrier, ClosePoint,
    };
    for point in [ClosePoint::BeforeClose, ClosePoint::AfterCloseAck] {
        let fixture = Fixture::new().await;
        let barrier = CloseBarrier::new();
        fixture
            .storage
            .request_registry()
            .connection_closer_for_test()
            .pause_next(point, barrier.clone())
            .unwrap();
        let service = fixture.service.clone();
        let mutation = fixture.mutation("close-owned");
        let registration =
            tokio::spawn(async move { service.register(&id(10), id(TARGET), &mutation).await });
        tokio::time::timeout(Duration::from_secs(3), barrier.arrived())
            .await
            .unwrap();
        let committed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_request_registrations")
            .fetch_one(fixture.storage.pool_for_test())
            .await
            .unwrap();
        assert_eq!(committed, 1);
        registration.abort();
        assert!(registration.await.unwrap_err().is_cancelled());
        {
            let mut first = Box::pin(fixture.storage.close());
            assert!(
                std::future::poll_fn(|cx| std::task::Poll::Ready(
                    first.as_mut().poll(cx).is_pending()
                ))
                .await
            );
        }
        let mut second = Box::pin(fixture.storage.close());
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(
                second.as_mut().poll(cx).is_pending()
            ))
            .await
        );
        assert!(!fixture.storage.registry_close_status().is_complete());
        barrier.release();
        tokio::time::timeout(Duration::from_secs(3), second)
            .await
            .unwrap();
        assert!(fixture.storage.registry_close_status().is_complete());
        #[cfg(windows)]
        assert_registry_file_exclusive(&fixture);
    }
}

#[tokio::test]
async fn registry_connection_close_report_failure_preserves_committed_registration() {
    let fixture = Fixture::new().await;
    fixture
        .storage
        .request_registry()
        .connection_closer_for_test()
        .fail_next_result_after_ack()
        .unwrap();
    let registration = fixture
        .service
        .register(
            &id(10),
            id(TARGET),
            &fixture.mutation("committed-close-report"),
        )
        .await
        .unwrap();
    assert!(
        matches!(fixture.service.query_request(&id(10), id(TARGET), registration.id()).await.unwrap(),
        RequestFact::Registered(fact) if fact.id() == registration.id())
    );
    fixture.storage.close().await;
    fixture.storage_scope.stop_and_observe().await;
    assert_eq!(
        fixture.storage.registry_close_status(),
        crate::session_storage::ConnectionCloseStatus::Unconfirmed
    );
    #[cfg(windows)]
    assert_registry_file_exclusive(&fixture);
}

#[tokio::test]
async fn registry_cancelled_raw_begin_is_rolled_back_and_physically_closed() {
    use crate::application::request_registry::{
        CurrentRequestActor, OperationContext, RequestRegistryRepository,
    };
    let fixture = Fixture::new().await;
    let mutation = fixture.mutation("cancel-transaction");
    let registration = fixture
        .service
        .register(&id(10), id(TARGET), &mutation)
        .await
        .unwrap();
    let repository = fixture.storage.request_registry();
    let barrier = Barrier::new();
    repository
        .faults_for_test()
        .arm(
            Point::BeforeCommit(Operation::Begin),
            Action::Pause(barrier.clone()),
        )
        .unwrap();
    let current = CurrentRequestActor::new(
        ActorContext {
            client_id: id(10),
            auth_generation: 1,
        },
        id(TARGET),
        NativeSessionKey::from_config(Some(id(1)), "synthetic-native").unwrap(),
        Some(QueueAction::Submit),
    );
    let task = tokio::spawn(async move {
        repository
            .begin_consume(
                &current,
                registration.id(),
                registration.intent(),
                id(90),
                &OperationContext {
                    runtime: RuntimeEpoch(id(91)),
                    observed_at_ms: 0,
                },
                &RegistryDeadline::new(Instant::now() + Duration::from_secs(3)),
            )
            .await
    });
    barrier.arrived().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    fixture.storage.close().await;
    fixture.storage_scope.stop_and_observe().await;
    assert!(fixture.storage.registry_close_status().is_complete());
    #[cfg(windows)]
    assert_registry_file_exclusive(&fixture);
    let raw_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &fixture.area.database(),
        fixture.area.clone(),
    );
    raw_scope.start_raw().await;
    let mut independent = sqlx::SqliteConnection::connect_with(&raw_scope.options())
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_request_operations")
        .fetch_one(&mut independent)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::Connection::close(independent).await.unwrap();
    raw_scope.stop_and_observe().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registry_physical_close_immediately_follows_concurrent_normal_finishes() {
    let mut tasks = Vec::new();
    for _ in 0..4 {
        tasks.push(tokio::spawn(async {
            for _ in 0..8 {
                let fixture = Fixture::new().await;
                fixture
                    .service
                    .register(&id(10), id(TARGET), &fixture.mutation("physical-close"))
                    .await
                    .unwrap();
                fixture.storage.close().await;
                fixture.storage_scope.stop_and_observe().await;
                assert!(fixture.storage.registry_close_status().is_complete());
                #[cfg(windows)]
                assert_registry_file_exclusive(&fixture);
            }
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
}

#[tokio::test]
async fn registry_storage_drop_seals_new_lease_without_claiming_async_cleanup() {
    use crate::application::request_registry::{CurrentRequestActor, RequestRegistryRepository};
    let Fixture {
        storage,
        service,
        driver,
        environment,
        area,
        storage_scope,
        ..
    } = Fixture::new().await;
    let repository = storage.request_registry();
    let pool = storage.pool_for_test().clone();
    let reader = CurrentRequestActor::new(
        ActorContext {
            client_id: id(10),
            auth_generation: 1,
        },
        id(TARGET),
        NativeSessionKey::from_config(Some(id(1)), "synthetic-native").unwrap(),
        None,
    );
    drop(storage);
    assert_eq!(
        repository
            .read(
                &reader,
                id(TARGET),
                RegistrationId::new(id(99)),
                &RegistryDeadline::new(Instant::now() + Duration::from_secs(3))
            )
            .await
            .err(),
        Some(RegistryError::NodeStopping)
    );
    assert!(
        service
            .shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(3)
            ))
            .await
            .joined
    );
    repository.close_connections().await;
    assert!(repository.close_status().is_complete());
    pool.close().await;
    storage_scope.stop_and_observe().await;
    drop(service);
    drop(driver);
    drop(environment);
    drop(repository);
    drop(area);
}

#[tokio::test]
async fn registry_pending_connection_can_be_dropped_on_a_thread_without_tokio_context() {
    use crate::application::request_registry::{
        CurrentRequestActor, OperationContext, RequestRegistryRepository,
    };
    let fixture = Fixture::new().await;
    let registration = fixture
        .service
        .register(
            &id(10),
            id(TARGET),
            &fixture.mutation("outside-runtime-drop"),
        )
        .await
        .unwrap();
    let repository = fixture.storage.request_registry();
    let barrier = Barrier::new();
    repository
        .faults_for_test()
        .arm(
            Point::BeforeCommit(Operation::Begin),
            Action::Pause(barrier.clone()),
        )
        .unwrap();
    let current = CurrentRequestActor::new(
        ActorContext {
            client_id: id(10),
            auth_generation: 1,
        },
        id(TARGET),
        NativeSessionKey::from_config(Some(id(1)), "synthetic-native").unwrap(),
        Some(QueueAction::Submit),
    );
    let mut operation = Box::pin(async move {
        repository
            .begin_consume(
                &current,
                registration.id(),
                registration.intent(),
                id(88),
                &OperationContext {
                    runtime: RuntimeEpoch(id(89)),
                    observed_at_ms: 0,
                },
                &RegistryDeadline::new(Instant::now() + Duration::from_secs(3)),
            )
            .await
    });
    tokio::select! {
        ()=barrier.arrived()=>{},
        _=operation.as_mut()=>panic!("operation must reach the real before-commit pending point"),
    }
    std::thread::spawn(move || drop(operation)).join().unwrap();
    fixture.storage.close().await;
    fixture.storage_scope.stop_and_observe().await;
    assert!(fixture.storage.registry_close_status().is_complete());
    #[cfg(windows)]
    assert_registry_file_exclusive(&fixture);
}

#[tokio::test]
async fn configuration_proofs_require_the_original_registry_owner() {
    use crate::application::shared_views::budget::ReadReason;
    use crate::domain::native_dispatch::DispatchError;
    let a = Fixture::new().await;
    let b = Fixture::new().await;
    let own_scope = a.storage.request_registry().owner_scope();
    let foreign_scope = b.storage.request_registry().owner_scope();
    let read = a
        .service
        .prepare_native_read(&id(10), id(TARGET), ReadReason::FirstOpen)
        .await
        .unwrap();
    a.submit("registry-owner-binding").await;
    let accepted = a.driver.accepted_target(id(TARGET)).await.unwrap().unwrap();
    let observations = (
        read.verify_registry_owner(&own_scope),
        read.verify_registry_owner(&foreign_scope),
        accepted.verify_registry_owner(&own_scope),
        accepted.verify_registry_owner(&foreign_scope),
    );
    b.close().await;
    a.close().await;
    assert_eq!(
        observations,
        (
            Ok(()),
            Err(DispatchError::InvalidScope),
            Ok(()),
            Err(DispatchError::InvalidScope)
        )
    );
    assert_eq!(
        read.verify_registry_owner(&own_scope),
        Err(DispatchError::NodeStopping)
    );
    assert_eq!(
        accepted.verify_registry_owner(&own_scope),
        Err(DispatchError::NodeStopping)
    );
}
