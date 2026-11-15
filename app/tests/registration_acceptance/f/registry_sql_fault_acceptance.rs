//! Independent F tests, attached as a cfg(test) child of the real SQLite adapter.
//! The closed controller exercises real SQL transactions; no fake repository or owner claims.
mod fixtures;
mod queue_ports;
use super::{
    SqliteRequestRegistry,
    test_hooks::{Action, Barrier, Operation, Point},
};
use crate::{
    application::request_registry::{
        BeginRecord, ConsumeLease, CurrentRequestActor, OperationContext, OperationProbe,
        RegistryDeadline, RequestRegistryRepository,
    },
    domain::{
        ActorContext, EntityId,
        native::{NativeSessionKey, RuntimeEpoch},
        request_registry::{
            QueueAction, QueueOutcome, QueueRejection, RegisteredIntent, RegistrationFact,
            RegistrationId, RegistryError, RequestFact, StoredApplying,
        },
    },
    storage::Storage,
};
use fixtures::{Area, connect, fixture_base};
use sqlx::Connection;
use tokio::time::{Duration, Instant, timeout};

const fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
fn deadline() -> RegistryDeadline {
    RegistryDeadline::new(Instant::now() + Duration::from_secs(20))
}
fn native() -> NativeSessionKey {
    NativeSessionKey::from_config(Some(id(80)), "registry-fault-synthetic").unwrap()
}
fn context(action: Option<QueueAction>) -> CurrentRequestActor {
    CurrentRequestActor::new(
        ActorContext {
            client_id: id(1),
            auth_generation: 12,
        },
        id(81),
        native(),
        action,
    )
}
const fn at(runtime: u8) -> OperationContext {
    OperationContext {
        runtime: RuntimeEpoch(id(runtime)),
        observed_at_ms: -123,
    }
}
fn submit_intent() -> RegisteredIntent {
    RegisteredIntent::new(
        id(1),
        QueueAction::Submit,
        id(81),
        None,
        None,
        Some([7; 32]),
    )
    .unwrap()
}
struct Setup {
    storage: Storage,
    area: Area,
    repo: SqliteRequestRegistry,
}
impl Setup {
    async fn new() -> Self {
        let area = Area::new();
        fixture_base(&area, 3).await;
        let storage = Storage::open(&area.db()).await.unwrap();
        let mut c = connect(&area.db()).await;
        sqlx::query("INSERT INTO product_sources VALUES(?,'registry-fault-source')")
            .bind(id(80).encode())
            .execute(&mut c)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_targets VALUES(?,?, 'registry-fault-synthetic',NULL)")
            .bind(id(81).encode())
            .bind(id(80).encode())
            .execute(&mut c)
            .await
            .unwrap();
        c.close().await.unwrap();
        let repo = storage.request_registry();
        Self {
            storage,
            area,
            repo,
        }
    }
    async fn count(&self, table: &str) -> i64 {
        let mut c = connect(&self.area.db()).await;
        let count = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&mut c)
            .await
            .unwrap();
        c.close().await.unwrap();
        count
    }
    async fn phase(&self, request: RegistrationId) -> Option<String> {
        let mut c = connect(&self.area.db()).await;
        let phase =
            sqlx::query_scalar("SELECT phase FROM node_request_operations WHERE request_id=?")
                .bind(request.encode())
                .fetch_optional(&mut c)
                .await
                .unwrap();
        c.close().await.unwrap();
        phase
    }
    async fn register(&self, n: u8, intent: &RegisteredIntent) -> RegistrationFact {
        self.repo
            .register(
                &context(Some(intent.action())),
                intent,
                RegistrationId::new(id(n)),
                &at(90),
                &deadline(),
            )
            .await
            .unwrap()
    }
    async fn begin(
        &self,
        r: &RegistrationFact,
        operation: u8,
    ) -> (OperationProbe, ConsumeLease, StoredApplying) {
        let current = context(Some(r.intent().action()));
        let stamp = at(90);
        let probe =
            OperationProbe::new(r.id(), id(operation), &stamp, &current, r.intent().clone());
        let BeginRecord::Started(record) = self
            .repo
            .begin_consume(
                &current,
                r.id(),
                r.intent(),
                id(operation),
                &stamp,
                &deadline(),
            )
            .await
            .unwrap()
        else {
            panic!("first consume must create applying")
        };
        let lease = ConsumeLease::mint(&probe, record.clone()).unwrap();
        (probe, lease, record)
    }
    async fn close(&self) {
        self.storage.close().await;
    }
}

#[tokio::test]
async fn cl075_f01_register_insert_and_precommit_faults_rollback_and_are_once_only() {
    for point in [
        Point::RegisterInsert,
        Point::BeforeCommit(Operation::Register),
    ] {
        let setup = Setup::new().await;
        setup
            .repo
            .faults_for_test()
            .arm(point, Action::Fail)
            .unwrap();
        let intent = submit_intent();
        let result = setup
            .repo
            .register(
                &context(Some(QueueAction::Submit)),
                &intent,
                RegistrationId::new(id(100)),
                &at(90),
                &deadline(),
            )
            .await;
        assert_eq!(result.err(), Some(RegistryError::StorageUnavailable));
        assert_eq!(setup.count("node_request_registrations").await, 0);
        assert_eq!(setup.count("node_request_operations").await, 0);
        setup.register(100, &intent).await;
        assert_eq!(setup.count("node_request_registrations").await, 1);
        setup.close().await;
    }
}

#[tokio::test]
async fn cl075_f02_begin_insert_and_precommit_faults_leave_registration_unconsumed() {
    for point in [Point::BeginInsert, Point::BeforeCommit(Operation::Begin)] {
        let setup = Setup::new().await;
        let r = setup.register(100, &submit_intent()).await;
        setup
            .repo
            .faults_for_test()
            .arm(point, Action::Fail)
            .unwrap();
        assert_eq!(
            setup
                .repo
                .begin_consume(
                    &context(Some(QueueAction::Submit)),
                    r.id(),
                    r.intent(),
                    id(110),
                    &at(90),
                    &deadline()
                )
                .await
                .err(),
            Some(RegistryError::StorageUnavailable)
        );
        assert_eq!(setup.phase(r.id()).await, None);
        assert_eq!(setup.count("node_request_registrations").await, 1);
        let (_, _, stored) = setup.begin(&r, 110).await;
        assert_eq!(stored.registration, r);
        setup.close().await;
    }
}

#[tokio::test]
async fn cl076_f03_settle_update_and_precommit_faults_preserve_applying_then_same_lease_settles() {
    for point in [Point::SettleUpdate, Point::BeforeCommit(Operation::Settle)] {
        let setup = Setup::new().await;
        let r = setup.register(100, &submit_intent()).await;
        let (probe, lease, stored) = setup.begin(&r, 110).await;
        let outcome = QueueOutcome::Queued {
            item: r.id().entity(),
            revision: 1,
        };
        setup
            .repo
            .faults_for_test()
            .arm(point, Action::Fail)
            .unwrap();
        assert_eq!(
            setup
                .repo
                .settle(&lease, &outcome, &at(90), &deadline())
                .await
                .err(),
            Some(RegistryError::StorageUnavailable)
        );
        assert_eq!(setup.phase(r.id()).await.as_deref(), Some("applying"));
        assert_eq!(
            setup
                .repo
                .inspect_operation(&probe, &deadline())
                .await
                .unwrap(),
            RequestFact::Applying(stored)
        );
        let fact = setup
            .repo
            .settle(&lease, &outcome, &at(90), &deadline())
            .await
            .unwrap();
        assert!(matches!(fact,RequestFact::Settled{outcome:o,..} if o==outcome));
        setup.close().await;
    }
}

#[tokio::test]
async fn cl075_f04_real_aftercommit_faults_expose_durable_facts_not_second_consume_right() {
    let setup = Setup::new().await;
    let intent = submit_intent();
    let registration = RegistrationId::new(id(100));
    setup
        .repo
        .faults_for_test()
        .arm(Point::AfterCommit(Operation::Register), Action::Unconfirmed)
        .unwrap();
    assert_eq!(
        setup
            .repo
            .register(
                &context(Some(QueueAction::Submit)),
                &intent,
                registration,
                &at(90),
                &deadline()
            )
            .await
            .err(),
        Some(RegistryError::CommitUnconfirmed)
    );
    assert_eq!(setup.count("node_request_registrations").await, 1);
    let registered = setup
        .repo
        .read(&context(None), id(81), registration, &deadline())
        .await
        .unwrap();
    let RequestFact::Registered(r) = registered else {
        panic!("lost registration acknowledgment must leave Registered")
    };
    let current = context(Some(QueueAction::Submit));
    let stamp = at(90);
    let probe = OperationProbe::new(r.id(), id(110), &stamp, &current, intent.clone());
    setup
        .repo
        .faults_for_test()
        .arm(Point::AfterCommit(Operation::Begin), Action::Unconfirmed)
        .unwrap();
    assert_eq!(
        setup
            .repo
            .begin_consume(&current, r.id(), &intent, id(110), &stamp, &deadline())
            .await
            .err(),
        Some(RegistryError::CommitUnconfirmed)
    );
    assert_eq!(setup.phase(r.id()).await.as_deref(), Some("applying"));
    let inspected = setup
        .repo
        .inspect_operation(&probe, &deadline())
        .await
        .unwrap();
    let RequestFact::Applying(stored) = inspected else {
        panic!("expected full applying inspection")
    };
    assert_eq!(stored.registration, r);
    assert_eq!(stored.operation, id(110));
    assert_eq!(stored.runtime, stamp.runtime);
    assert_eq!(stored.admission_auth_generation, 12);
    assert_eq!(stored.begun_at_ms, -123);
    let repeated = setup
        .repo
        .begin_consume(&current, r.id(), &intent, id(111), &stamp, &deadline())
        .await
        .unwrap();
    assert!(matches!(repeated,BeginRecord::Existing(RequestFact::Applying(s)) if s==stored));
    let lease = ConsumeLease::mint(&probe, stored).unwrap();
    let outcome = QueueOutcome::Queued {
        item: r.id().entity(),
        revision: 1,
    };
    setup
        .repo
        .faults_for_test()
        .arm(Point::AfterCommit(Operation::Settle), Action::Unconfirmed)
        .unwrap();
    assert_eq!(
        setup
            .repo
            .settle(&lease, &outcome, &stamp, &deadline())
            .await
            .err(),
        Some(RegistryError::CommitUnconfirmed)
    );
    assert_eq!(setup.phase(r.id()).await.as_deref(), Some("settled"));
    assert!(
        matches!(setup.repo.inspect_operation(&probe,&deadline()).await.unwrap(),RequestFact::Settled{outcome:o,..} if o==outcome)
    );
    setup.close().await;
}

#[tokio::test]
async fn cl092_f05_node_settle_after_revoke_is_idempotent_and_conflicting_result_rejected() {
    let setup = Setup::new().await;
    let r = setup.register(100, &submit_intent()).await;
    let (_, lease, _) = setup.begin(&r, 110).await;
    let mut c = connect(&setup.area.db()).await;
    sqlx::query("UPDATE clients SET status='revoked',auth_generation=13 WHERE client_id=?")
        .bind(id(1).encode())
        .execute(&mut c)
        .await
        .unwrap();
    c.close().await.unwrap();
    let outcome = QueueOutcome::Queued {
        item: r.id().entity(),
        revision: 1,
    };
    let original = setup
        .repo
        .settle(&lease, &outcome, &at(90), &deadline())
        .await
        .unwrap();
    assert_eq!(
        setup
            .repo
            .settle(&lease, &outcome, &at(90), &deadline())
            .await
            .unwrap(),
        original
    );
    assert_eq!(
        setup
            .repo
            .settle(
                &lease,
                &QueueOutcome::Rejected(QueueRejection::Full),
                &at(90),
                &deadline()
            )
            .await
            .err(),
        Some(RegistryError::Conflict)
    );
    assert_eq!(
        setup
            .repo
            .read(&context(None), id(81), r.id(), &deadline())
            .await
            .err(),
        Some(RegistryError::InvalidActor)
    );
    setup.close().await;
}

#[tokio::test]
async fn cl082_f06_startup_faults_only_interrupt_old_applying_without_replay() {
    for point in [
        Point::StartupUpdate,
        Point::BeforeCommit(Operation::Startup),
        Point::AfterCommit(Operation::Startup),
    ] {
        let setup = Setup::new().await;
        let r = setup.register(100, &submit_intent()).await;
        let (_, lease, _) = setup.begin(&r, 110).await;
        assert_eq!(
            setup
                .repo
                .interrupt_previous_runtime(at(90).runtime, &at(90), &deadline())
                .await
                .unwrap(),
            0
        );
        assert_eq!(setup.phase(r.id()).await.as_deref(), Some("applying"));
        setup
            .repo
            .faults_for_test()
            .arm(
                point,
                if matches!(point, Point::AfterCommit(_)) {
                    Action::Unconfirmed
                } else {
                    Action::Fail
                },
            )
            .unwrap();
        assert!(
            setup
                .repo
                .interrupt_previous_runtime(at(91).runtime, &at(91), &deadline())
                .await
                .is_err()
        );
        let committed = matches!(point, Point::AfterCommit(_));
        assert_eq!(
            setup.phase(r.id()).await.as_deref(),
            Some(if committed { "interrupted" } else { "applying" })
        );
        let changed = setup
            .repo
            .interrupt_previous_runtime(at(91).runtime, &at(91), &deadline())
            .await
            .unwrap();
        assert_eq!(changed, u64::from(!committed));
        assert_eq!(setup.phase(r.id()).await.as_deref(), Some("interrupted"));
        assert!(
            setup
                .repo
                .settle(
                    &lease,
                    &QueueOutcome::Queued {
                        item: r.id().entity(),
                        revision: 1
                    },
                    &at(90),
                    &deadline()
                )
                .await
                .is_err()
        );
        assert_eq!(setup.count("node_request_operations").await, 1);
        setup.close().await;
    }
}

#[tokio::test]
async fn cl075_f07_cancelled_detached_transaction_rolls_back_before_commit_but_not_after_commit() {
    for after_commit in [false, true] {
        let setup = Setup::new().await;
        let barrier = Barrier::new();
        setup
            .repo
            .faults_for_test()
            .arm(
                if after_commit {
                    Point::AfterCommit(Operation::Register)
                } else {
                    Point::RegisterInsert
                },
                Action::Pause(barrier.clone()),
            )
            .unwrap();
        let repo = setup.repo.clone();
        let task = tokio::spawn(async move {
            repo.register(
                &context(Some(QueueAction::Submit)),
                &submit_intent(),
                RegistrationId::new(id(100)),
                &at(90),
                &deadline(),
            )
            .await
        });
        timeout(Duration::from_secs(20), barrier.arrived())
            .await
            .unwrap();
        task.abort();
        let _ = task.await;
        barrier.release();
        let mut c = timeout(Duration::from_secs(20), connect(&setup.area.db()))
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_request_registrations")
            .fetch_one(&mut c)
            .await
            .unwrap();
        assert_eq!(count, i64::from(after_commit));
        c.close().await.unwrap();
        setup.register(101, &submit_intent()).await;
        assert_eq!(
            setup.count("node_request_registrations").await,
            1 + i64::from(after_commit)
        );
        setup.close().await;
    }
}

#[tokio::test]
async fn cl018_f08_legal_ddl_shape_but_wrong_action_result_is_corrupt_on_adapter_read() {
    let setup = Setup::new().await;
    for (offset, action, result) in [
        (0, QueueAction::Submit, "resumed"),
        (1, QueueAction::Edit, "queued"),
        (2, QueueAction::Cancel, "edited"),
        (3, QueueAction::Prioritize, "cancelled"),
        (4, QueueAction::Resume, "rejected"),
    ] {
        let (item, revision, digest) = match action {
            QueueAction::Submit => (None, None, Some([7; 32])),
            QueueAction::Edit => (Some(id(82)), Some(1), Some([7; 32])),
            QueueAction::Cancel | QueueAction::Prioritize => (Some(id(82)), Some(1), None),
            QueueAction::Resume => (None, None, None),
        };
        let intent = RegisteredIntent::new(id(1), action, id(81), item, revision, digest).unwrap();
        let r = setup.register(100 + offset, &intent).await;
        setup.begin(&r, 110 + offset).await;
        let mut c = connect(&setup.area.db()).await;
        sqlx::query("UPDATE node_request_operations SET phase='settled',resolved_at_ms=1,result_code=?,result_item_id=?,result_revision=?,restored_count=?,rejection_code=? WHERE request_id=?")
            .bind(result).bind(if matches!(result,"resumed"|"rejected"){None}else{Some(id(82).encode())}).bind(if matches!(result,"resumed"|"rejected"){None}else{Some(1_u64.to_be_bytes().to_vec())}).bind(if result=="resumed"{Some(0_i64)}else{None}).bind(if result=="rejected"{Some("full")}else{None}).bind(r.id().encode()).execute(&mut c).await.unwrap();
        c.close().await.unwrap();
        assert_eq!(
            setup
                .repo
                .read(&context(None), id(81), r.id(), &deadline())
                .await
                .err(),
            Some(RegistryError::CorruptState)
        );
    }
    setup.close().await;
}

#[tokio::test]
async fn cl080_f09_retired_detail_pruning_and_unknown_ids_never_restore_consumption() {
    let setup = Setup::new().await;
    let r = setup.register(100, &submit_intent()).await;
    let (_, lease, _) = setup.begin(&r, 110).await;
    setup
        .repo
        .settle(
            &lease,
            &QueueOutcome::Queued {
                item: r.id().entity(),
                revision: 1,
            },
            &at(90),
            &deadline(),
        )
        .await
        .unwrap();
    let protected = setup.register(101, &submit_intent()).await;
    let (protected_probe, _, protected_applying) = setup.begin(&protected, 120).await;
    let mut c = connect(&setup.area.db()).await;
    sqlx::query("UPDATE node_request_registrations SET retired=1 WHERE request_id=?")
        .bind(r.id().encode())
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("DELETE FROM node_request_operations WHERE request_id=?")
        .bind(r.id().encode())
        .execute(&mut c)
        .await
        .unwrap();
    c.close().await.unwrap();
    assert_eq!(
        setup
            .repo
            .read(&context(None), id(81), r.id(), &deadline())
            .await
            .unwrap(),
        RequestFact::Retired
    );
    assert_eq!(
        setup
            .repo
            .begin_consume(
                &context(Some(QueueAction::Submit)),
                r.id(),
                r.intent(),
                id(111),
                &at(90),
                &deadline()
            )
            .await
            .err(),
        Some(RegistryError::Retired)
    );
    assert_eq!(
        setup
            .repo
            .begin_consume(
                &context(Some(QueueAction::Submit)),
                RegistrationId::new(id(250)),
                &submit_intent(),
                id(112),
                &at(90),
                &deadline()
            )
            .await
            .err(),
        Some(RegistryError::UnknownRegistration)
    );
    assert_eq!(
        setup
            .repo
            .inspect_operation(&protected_probe, &deadline())
            .await
            .unwrap(),
        RequestFact::Applying(protected_applying)
    );
    assert_eq!(
        setup.phase(protected.id()).await.as_deref(),
        Some("applying")
    );
    assert_eq!(setup.count("node_request_registrations").await, 2);
    assert_eq!(setup.count("node_request_operations").await, 1);
    setup.close().await;
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep the complete frozen action/outcome table visible for independent review."
)]
fn cl018_f10_closed_action_outcome_matrix_preserves_full_revision_and_rejects_unlisted_reasons() {
    let request = RegistrationId::new(id(100));
    let item = id(82);
    let outcomes = [
        QueueOutcome::Queued {
            item: request.entity(),
            revision: 1,
        },
        QueueOutcome::DirectSelected {
            item: request.entity(),
            revision: 1,
        },
        QueueOutcome::Edited { item, revision: 2 },
        QueueOutcome::Cancelled { item, revision: 1 },
        QueueOutcome::Prioritized { item, revision: 1 },
        QueueOutcome::Resumed { restored: 0 },
        QueueOutcome::Resumed { restored: 3 },
        QueueOutcome::Resumed { restored: 4 },
    ];
    for action in [
        QueueAction::Submit,
        QueueAction::Edit,
        QueueAction::Cancel,
        QueueAction::Prioritize,
        QueueAction::Resume,
    ] {
        let (bound, version, digest) = match action {
            QueueAction::Submit => (None, None, Some([7; 32])),
            QueueAction::Edit => (Some(item), Some(1), Some([7; 32])),
            QueueAction::Cancel | QueueAction::Prioritize => (Some(item), Some(1), None),
            QueueAction::Resume => (None, None, None),
        };
        let intent = RegisteredIntent::new(id(1), action, id(81), bound, version, digest).unwrap();
        for value in outcomes {
            let allowed = matches!(
                (action, value),
                (
                    QueueAction::Submit,
                    QueueOutcome::Queued { .. } | QueueOutcome::DirectSelected { .. }
                ) | (QueueAction::Edit, QueueOutcome::Edited { .. })
                    | (QueueAction::Cancel, QueueOutcome::Cancelled { .. })
                    | (QueueAction::Prioritize, QueueOutcome::Prioritized { .. })
                    | (
                        QueueAction::Resume,
                        QueueOutcome::Resumed { restored: 0..=3 }
                    )
            );
            assert_eq!(
                intent.validate_outcome(request, value).is_ok(),
                allowed,
                "{action:?}/{value:?}"
            );
        }
        for rejection in [
            QueueRejection::Full,
            QueueRejection::DuplicateId,
            QueueRejection::NotFound,
            QueueRejection::RevisionChanged,
            QueueRejection::RevisionExhausted,
            QueueRejection::DispatchStarted,
            QueueRejection::Suspended,
            QueueRejection::Busy,
            QueueRejection::SequenceExhausted,
        ] {
            let allowed = match action {
                QueueAction::Submit => matches!(
                    rejection,
                    QueueRejection::Full
                        | QueueRejection::DuplicateId
                        | QueueRejection::SequenceExhausted
                ),
                QueueAction::Edit => matches!(
                    rejection,
                    QueueRejection::DispatchStarted
                        | QueueRejection::NotFound
                        | QueueRejection::RevisionChanged
                        | QueueRejection::RevisionExhausted
                ),
                QueueAction::Cancel => matches!(
                    rejection,
                    QueueRejection::DispatchStarted
                        | QueueRejection::NotFound
                        | QueueRejection::RevisionChanged
                ),
                QueueAction::Prioritize => matches!(
                    rejection,
                    QueueRejection::DispatchStarted
                        | QueueRejection::NotFound
                        | QueueRejection::RevisionChanged
                        | QueueRejection::Suspended
                ),
                QueueAction::Resume => false,
            };
            assert_eq!(
                intent
                    .validate_outcome(request, QueueOutcome::Rejected(rejection))
                    .is_ok(),
                allowed
            );
        }
    }
    let max = RegisteredIntent::new(
        id(1),
        QueueAction::Edit,
        id(81),
        Some(item),
        Some(u64::MAX),
        Some([7; 32]),
    )
    .unwrap();
    assert_eq!(max.expected_revision(), Some(u64::MAX));
    assert!(
        max.validate_outcome(request, QueueOutcome::Edited { item, revision: 0 })
            .is_err()
    );
    assert!(
        max.validate_outcome(
            request,
            QueueOutcome::Rejected(QueueRejection::RevisionExhausted)
        )
        .is_ok()
    );
}

#[tokio::test]
async fn cl017_f11_two_real_pools_compare_and_set_one_applying_operation() {
    let setup = Setup::new().await;
    let registration = setup.register(100, &submit_intent()).await;
    let second_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(setup.area.db())
                .foreign_keys(true),
        )
        .await
        .unwrap();
    let second = SqliteRequestRegistry::new(second_pool.clone());
    let (a, b) = tokio::join!(
        async {
            setup
                .repo
                .begin_consume(
                    &context(Some(QueueAction::Submit)),
                    registration.id(),
                    registration.intent(),
                    id(110),
                    &at(90),
                    &deadline(),
                )
                .await
                .unwrap()
        },
        async {
            second
                .begin_consume(
                    &context(Some(QueueAction::Submit)),
                    registration.id(),
                    registration.intent(),
                    id(111),
                    &at(90),
                    &deadline(),
                )
                .await
                .unwrap()
        }
    );
    let ((BeginRecord::Started(owned), BeginRecord::Existing(repeated))
    | (BeginRecord::Existing(repeated), BeginRecord::Started(owned))) = (a, b)
    else {
        panic!("two SQL pools did not yield exactly one Started")
    };
    assert_eq!(repeated, RequestFact::Applying(owned));
    assert_eq!(setup.count("node_request_operations").await, 1);
    second_pool.close().await;
    setup.close().await;
}

#[tokio::test]
async fn cl081_f12_more_than_old_ten_thousand_metadata_records_do_not_create_new_cap() {
    let setup = Setup::new().await;
    let mut c = connect(&setup.area.db()).await;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut c)
        .await
        .unwrap();
    for n in 1_u128..=10_001 {
        let request = EntityId::from_bytes(n.to_be_bytes()).encode();
        let operation = EntityId::from_bytes((n + 20_000).to_be_bytes()).encode();
        sqlx::query("INSERT INTO node_request_registrations(request_id,intent_version,actor_id,register_auth_generation,action,target_id,registered_at_ms) VALUES(?,1,?,12,'input.resume',?,0)").bind(&request).bind(id(1).encode()).bind(id(81).encode()).execute(&mut c).await.unwrap();
        sqlx::query("INSERT INTO node_request_operations(request_id,operation_id,runtime_epoch,admission_auth_generation,phase,begun_at_ms,resolved_at_ms,result_code,restored_count) VALUES(?,?,?,12,'settled',0,0,'resumed',0)").bind(request).bind(operation).bind(id(90).encode()).execute(&mut c).await.unwrap();
    }
    sqlx::query("COMMIT").execute(&mut c).await.unwrap();
    c.close().await.unwrap();
    let registration = setup.register(100, &submit_intent()).await;
    setup.begin(&registration, 110).await;
    assert_eq!(setup.count("node_request_registrations").await, 10_002);
    assert_eq!(setup.count("node_request_operations").await, 10_002);
    setup.close().await;
}

#[tokio::test]
async fn cl104_f13_runtime_and_registration_random_failures_and_collision_preserve_identity() {
    use crate::application::{
        queue_owner::BlockedEnvironment,
        request_registry::{
            NoAudit, ValidatedMutation,
            random_hooks::{Action as RandomAction, Point as RandomPoint},
        },
    };
    use std::sync::Arc;
    let setup = Setup::new().await;
    let random = setup.storage.queue_random_faults_for_test();
    random
        .arm(RandomPoint::Runtime, RandomAction::Unavailable)
        .unwrap();
    assert_eq!(
        setup
            .storage
            .start_queue_service_with_audit_for_fixture(
                queue_ports::Authority,
                queue_ports::Validator,
                queue_ports::Clock,
                Arc::new(BlockedEnvironment),
                Arc::new(NoAudit)
            )
            .await
            .err(),
        Some(RegistryError::RandomUnavailable)
    );
    let (service, _) = setup
        .storage
        .start_queue_service_with_audit_for_fixture(
            queue_ports::Authority,
            queue_ports::Validator,
            queue_ports::Clock,
            Arc::new(BlockedEnvironment),
            Arc::new(NoAudit),
        )
        .await
        .unwrap();
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 12,
    };
    let mutation =
        ValidatedMutation::Submit(service.validate_body(b"random-synthetic-body").unwrap());
    random
        .arm(RandomPoint::Registration, RandomAction::Unavailable)
        .unwrap();
    assert_eq!(
        service.register(&actor, id(81), &mutation).await.err(),
        Some(RegistryError::RandomUnavailable)
    );
    assert_eq!(setup.count("node_request_registrations").await, 0);
    let first = service.register(&actor, id(81), &mutation).await.unwrap();
    random
        .arm(
            RandomPoint::Registration,
            RandomAction::Value(first.id().entity()),
        )
        .unwrap();
    let next = service.register(&actor, id(81), &mutation).await.unwrap();
    assert_ne!(first.id(), next.id());
    assert_eq!(
        service
            .query_request(&actor, id(81), first.id())
            .await
            .unwrap(),
        RequestFact::Registered(first)
    );
    assert_eq!(setup.count("node_request_registrations").await, 2);
    assert_eq!(setup.count("node_request_operations").await, 0);
    let report = service.shutdown(deadline()).await;
    assert!(report.joined);
    setup.close().await;
}

#[tokio::test]
async fn cl104_f14_scope_and_native_binding_are_checked_by_real_repository_not_plain_metadata() {
    let setup = Setup::new().await;
    let intent = submit_intent();
    let request = RegistrationId::new(id(100));
    assert!(
        setup
            .repo
            .register(&context(None), &intent, request, &at(90), &deadline())
            .await
            .is_err()
    );
    let wrong = CurrentRequestActor::new(
        ActorContext {
            client_id: id(1),
            auth_generation: 12,
        },
        id(81),
        NativeSessionKey::from_config(Some(id(99)), "different-native").unwrap(),
        Some(QueueAction::Submit),
    );
    assert_eq!(
        setup
            .repo
            .register(&wrong, &intent, request, &at(90), &deadline())
            .await
            .err(),
        Some(RegistryError::InvalidTarget)
    );
    assert_eq!(setup.count("node_request_registrations").await, 0);
    let registration = setup.register(100, &intent).await;
    assert_eq!(
        setup
            .repo
            .read(&context(None), id(82), registration.id(), &deadline())
            .await
            .err(),
        Some(RegistryError::InvalidTarget)
    );
    assert_eq!(setup.count("node_request_operations").await, 0);
    setup.close().await;
}

#[tokio::test]
async fn cl018_f15_full_u64_revision_survives_real_adapter_read_and_outcome_binding() {
    let setup = Setup::new().await;
    let intent = RegisteredIntent::new(
        id(1),
        QueueAction::Edit,
        id(81),
        Some(id(82)),
        Some(u64::MAX),
        Some([7; 32]),
    )
    .unwrap();
    let registration = setup.register(100, &intent).await;
    let fact = setup
        .repo
        .read(&context(None), id(81), registration.id(), &deadline())
        .await
        .unwrap();
    assert_eq!(
        fact.registration().unwrap().intent().expected_revision(),
        Some(u64::MAX)
    );
    let (_, lease, _) = setup.begin(&registration, 110).await;
    assert_eq!(
        setup
            .repo
            .settle(
                &lease,
                &QueueOutcome::Edited {
                    item: id(82),
                    revision: 0
                },
                &at(90),
                &deadline()
            )
            .await
            .err(),
        Some(RegistryError::InvalidIntent)
    );
    assert_eq!(
        setup.phase(registration.id()).await.as_deref(),
        Some("applying")
    );
    let settled = setup
        .repo
        .settle(
            &lease,
            &QueueOutcome::Rejected(QueueRejection::RevisionExhausted),
            &at(90),
            &deadline(),
        )
        .await
        .unwrap();
    assert!(matches!(
        settled,
        RequestFact::Settled {
            outcome: QueueOutcome::Rejected(QueueRejection::RevisionExhausted),
            ..
        }
    ));
    setup.close().await;
}

#[tokio::test]
async fn cl092_f16_caller_drop_during_real_begin_transaction_leaves_node_owned_admission() {
    use crate::application::{
        queue_owner::{ConsumeReply, QueueRead},
        request_registry::{NoAudit, ValidatedMutation},
    };
    use std::sync::Arc;
    let setup = Setup::new().await;
    let (service, _) = setup
        .storage
        .start_queue_service_with_audit_for_fixture(
            queue_ports::Authority,
            queue_ports::Validator,
            queue_ports::Clock,
            Arc::new(queue_ports::Environment),
            Arc::new(NoAudit),
        )
        .await
        .unwrap();
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 12,
    };
    let mutation =
        ValidatedMutation::Submit(service.validate_body(b"owned-before-real-commit").unwrap());
    let r = service.register(&actor, id(81), &mutation).await.unwrap();
    let barrier = Barrier::new();
    setup
        .repo
        .faults_for_test()
        .arm(
            Point::BeforeCommit(Operation::Begin),
            Action::Pause(barrier.clone()),
        )
        .unwrap();
    let owned = service.clone();
    let request = r.id();
    let waiter =
        tokio::spawn(async move { owned.consume(&actor, id(81), request, mutation).await });
    timeout(Duration::from_secs(20), barrier.arrived())
        .await
        .unwrap();
    assert_eq!(setup.phase(request).await, None);
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    barrier.release();
    let fact = timeout(Duration::from_secs(20), async {
        loop {
            let fact = service
                .query_request(&actor, id(81), request)
                .await
                .unwrap();
            if matches!(fact, RequestFact::Settled { .. }) {
                break fact;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        matches!(fact, RequestFact::Settled { outcome: QueueOutcome::Queued { item, revision: 1 }, .. } if item == request.entity())
    );
    let ids = timeout(Duration::from_secs(20), async {
        loop {
            let stable = service
                .inspect_queue(&actor, id(81), |read| match read {
                    QueueRead::Stable { queue } => Some(
                        queue
                            .waiting()
                            .map(crate::domain::input_queue::QueueEntry::id)
                            .collect::<Vec<_>>(),
                    ),
                    QueueRead::Processing { .. } => None,
                })
                .await
                .unwrap();
            if let Some(ids) = stable {
                break ids;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(ids, vec![request.entity()]);
    let retry =
        ValidatedMutation::Submit(service.validate_body(b"owned-before-real-commit").unwrap());
    assert!(matches!(
        service.consume(&actor, id(81), request, retry).await,
        ConsumeReply::Resolved {
            fact: RequestFact::Settled { .. },
            returned: Some(_)
        }
    ));
    assert_eq!(setup.count("node_request_operations").await, 1);
    assert!(service.shutdown(deadline()).await.joined);
    setup.close().await;
}

#[tokio::test]
async fn cl110_f17_evt01_true_begin_precommit_failure_event_precedes_new_d() {
    use crate::application::{
        queue_owner::{ConsumeReply, QueueOwnerEvent, QueueRead},
        request_registry::{NoAudit, ValidatedMutation},
    };
    use std::sync::Arc;
    let setup = Setup::new().await;
    let (service, driver) = setup
        .storage
        .start_queue_service_with_audit_for_fixture(
            queue_ports::Authority,
            queue_ports::Validator,
            queue_ports::Clock,
            Arc::new(queue_ports::Environment),
            Arc::new(NoAudit),
        )
        .await
        .unwrap();
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 12,
    };
    let mut ids = Vec::new();
    for body in [b"old-X".as_slice(), b"old-Y".as_slice()] {
        let m = ValidatedMutation::Submit(service.validate_body(body).unwrap());
        let r = service.register(&actor, id(81), &m).await.unwrap();
        ids.push(r.id().entity());
        assert!(matches!(
            service.consume(&actor, id(81), r.id(), m).await,
            ConsumeReply::Resolved {
                fact: RequestFact::Settled {
                    outcome: QueueOutcome::Queued { .. },
                    ..
                },
                ..
            }
        ));
    }
    let work = driver
        .open_external_work(id(81), queue_ports::resource())
        .await
        .unwrap();
    let m = ValidatedMutation::Submit(service.validate_body(b"new-D").unwrap());
    let r = service.register(&actor, id(81), &m).await.unwrap();
    let request = r.id();
    ids.push(request.entity());
    let barrier = Barrier::new();
    setup
        .repo
        .faults_for_test()
        .arm(
            Point::BeforeCommit(Operation::Begin),
            Action::Pause(barrier.clone()),
        )
        .unwrap();
    let owned = service.clone();
    let waiter = tokio::spawn(async move { owned.consume(&actor, id(81), request, m).await });
    timeout(Duration::from_secs(20), barrier.arrived())
        .await
        .unwrap();
    assert_eq!(setup.phase(request).await, None);
    driver
        .observe(id(81), QueueOwnerEvent::ExternalForegroundFailure { work })
        .await
        .unwrap();
    barrier.release();
    assert!(matches!(
        waiter.await.unwrap(),
        ConsumeReply::Resolved {
            fact: RequestFact::Settled {
                outcome: QueueOutcome::Queued { .. },
                ..
            },
            ..
        }
    ));
    let projection = service
        .inspect_queue(&actor, id(81), |read| match read {
            QueueRead::Stable { queue } => (
                queue
                    .waiting()
                    .map(|e| (e.id(), e.suspended()))
                    .collect::<Vec<_>>(),
                queue.failure_suspended(),
                queue.active().is_none(),
            ),
            QueueRead::Processing { .. } => panic!("consume must have reliable settled result"),
        })
        .await
        .unwrap();
    assert_eq!(
        projection.0,
        vec![(ids[0], true), (ids[1], true), (ids[2], false)]
    );
    assert!(projection.1 && projection.2);
    assert_eq!(setup.phase(request).await.as_deref(), Some("settled"));
    assert_eq!(setup.count("node_request_operations").await, 3);
    assert!(service.shutdown(deadline()).await.joined);
    setup.close().await;
}

#[tokio::test]
async fn cl018_f18_full_action_result_table_is_enforced_by_actual_sql_settle_adapter() {
    let setup = Setup::new().await;
    let item = id(82);
    let mut serial = 100u8;
    for action in [
        QueueAction::Submit,
        QueueAction::Edit,
        QueueAction::Cancel,
        QueueAction::Prioritize,
        QueueAction::Resume,
    ] {
        let (bound, revision, digest) = match action {
            QueueAction::Submit => (None, None, Some([7; 32])),
            QueueAction::Edit => (Some(item), Some(1), Some([7; 32])),
            QueueAction::Cancel | QueueAction::Prioritize => (Some(item), Some(1), None),
            QueueAction::Resume => (None, None, None),
        };
        for case in 0..20 {
            let intent =
                RegisteredIntent::new(id(1), action, id(81), bound, revision, digest).unwrap();
            let r = setup.register(serial, &intent).await;
            let (probe, lease, stored) = setup.begin(&r, serial).await;
            let values = [
                QueueOutcome::Queued {
                    item: r.id().entity(),
                    revision: 1,
                },
                QueueOutcome::DirectSelected {
                    item: r.id().entity(),
                    revision: 1,
                },
                QueueOutcome::Edited { item, revision: 2 },
                QueueOutcome::Cancelled { item, revision: 1 },
                QueueOutcome::Prioritized { item, revision: 1 },
                QueueOutcome::Resumed { restored: 0 },
                QueueOutcome::Resumed { restored: 3 },
                QueueOutcome::Resumed { restored: 4 },
                QueueOutcome::Rejected(QueueRejection::Full),
                QueueOutcome::Rejected(QueueRejection::DuplicateId),
                QueueOutcome::Rejected(QueueRejection::NotFound),
                QueueOutcome::Rejected(QueueRejection::RevisionChanged),
                QueueOutcome::Rejected(QueueRejection::RevisionExhausted),
                QueueOutcome::Rejected(QueueRejection::DispatchStarted),
                QueueOutcome::Rejected(QueueRejection::Suspended),
                QueueOutcome::Rejected(QueueRejection::Busy),
                QueueOutcome::Rejected(QueueRejection::SequenceExhausted),
                QueueOutcome::Queued {
                    item: id(83),
                    revision: 1,
                },
                QueueOutcome::Edited { item, revision: 3 },
                QueueOutcome::Cancelled { item, revision: 2 },
            ];
            let expected = match action {
                QueueAction::Submit => matches!(case, 0 | 1 | 8 | 9 | 16),
                QueueAction::Edit => matches!(case, 2 | 10 | 11 | 12 | 13),
                QueueAction::Cancel => matches!(case, 3 | 10 | 11 | 13),
                QueueAction::Prioritize => matches!(case, 4 | 10 | 11 | 13 | 14),
                QueueAction::Resume => matches!(case, 5 | 6),
            };
            let value = values[case];
            let result = setup
                .repo
                .settle(&lease, &value, &at(90), &deadline())
                .await;
            if expected {
                assert!(
                    matches!(result.unwrap(),RequestFact::Settled {outcome,..} if outcome==value),
                    "{action:?}/{case}"
                );
                assert_eq!(setup.phase(r.id()).await.as_deref(), Some("settled"));
            } else {
                assert_eq!(
                    result.err(),
                    Some(RegistryError::InvalidIntent),
                    "{action:?}/{case}"
                );
                assert_eq!(setup.phase(r.id()).await.as_deref(), Some("applying"));
                assert_eq!(
                    setup
                        .repo
                        .inspect_operation(&probe, &deadline())
                        .await
                        .unwrap(),
                    RequestFact::Applying(stored)
                );
            }
            serial = serial.checked_add(1).unwrap();
        }
    }
    assert_eq!(setup.count("node_request_registrations").await, 100);
    assert_eq!(setup.count("node_request_operations").await, 100);
    setup.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reg_shutdown_06_joined_old_cancel_and_concurrent_close_cannot_release_new_runtime_claim() {
    use crate::application::{
        queue_owner::{ConsumeReply, QueueRead, RuntimeSupervisor, StartupStatus},
        request_registry::{NoAudit, ValidatedMutation},
    };
    use std::sync::Arc;
    let setup = Setup::new().await;
    let (old, _) = setup
        .storage
        .start_queue_service_with_audit_for_fixture(
            queue_ports::Authority,
            queue_ports::Validator,
            queue_ports::Clock,
            Arc::new(queue_ports::Environment),
            Arc::new(NoAudit),
        )
        .await
        .unwrap();
    assert!(old.shutdown(deadline()).await.joined);
    assert_eq!(old.startup_status(), StartupStatus::Stopped);
    let (current, _) = setup
        .storage
        .start_queue_service_with_audit_for_fixture(
            queue_ports::Authority,
            queue_ports::Validator,
            queue_ports::Clock,
            Arc::new(queue_ports::Environment),
            Arc::new(NoAudit),
        )
        .await
        .unwrap();
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 12,
    };
    let m = ValidatedMutation::Submit(current.validate_body(b"new-runtime-owned-body").unwrap());
    let r = current.register(&actor, id(81), &m).await.unwrap();
    assert!(matches!(
        current.consume(&actor, id(81), r.id(), m).await,
        ConsumeReply::Resolved {
            fact: RequestFact::Settled {
                outcome: QueueOutcome::Queued { .. },
                ..
            },
            ..
        }
    ));
    let mut closers = Vec::new();
    for _ in 0..32 {
        let old = old.clone();
        closers.push(tokio::spawn(async move {
            RuntimeSupervisor::cancel(&old);
            let report = old.shutdown(deadline()).await;
            assert!(report.joined);
            assert_eq!(old.startup_status(), StartupStatus::Stopped);
        }));
    }
    for _ in 0..16 {
        assert!(matches!(
            setup
                .storage
                .start_queue_service_with_audit_for_fixture(
                    queue_ports::Authority,
                    queue_ports::Validator,
                    queue_ports::Clock,
                    Arc::new(queue_ports::Environment),
                    Arc::new(NoAudit)
                )
                .await,
            Err(RegistryError::RuntimeAlreadyStarted)
        ));
        assert_eq!(current.startup_status(), StartupStatus::Running);
        assert!(matches!(
            current.query_request(&actor, id(81), r.id()).await.unwrap(),
            RequestFact::Settled { .. }
        ));
    }
    for closer in closers {
        closer.await.unwrap();
    }
    assert_eq!(old.startup_status(), StartupStatus::Stopped);
    assert_eq!(current.startup_status(), StartupStatus::Running);
    let ids = current
        .inspect_queue(&actor, id(81), |read| match read {
            QueueRead::Stable { queue } => queue
                .waiting()
                .map(crate::domain::input_queue::QueueEntry::id)
                .collect::<Vec<_>>(),
            QueueRead::Processing { .. } => panic!("old shutdown changed the new stable owner"),
        })
        .await
        .unwrap();
    assert_eq!(ids, vec![r.id().entity()]);
    assert_eq!(setup.count("node_request_operations").await, 1);
    assert!(current.shutdown(deadline()).await.joined);
    setup.close().await;
}

#[tokio::test]
async fn cl076_f20_actual_owner_settle_sql_rollback_only_retries_e_after_edit_d() {
    use crate::application::{
        queue_owner::{ConsumeReply, QueueRead},
        request_registry::{NoAudit, ValidatedMutation},
    };
    use std::sync::Arc;
    for point in [Point::SettleUpdate, Point::BeforeCommit(Operation::Settle)] {
        let setup = Setup::new().await;
        let (service, _) = setup
            .storage
            .start_queue_service_with_audit_for_fixture(
                queue_ports::Authority,
                queue_ports::Validator,
                queue_ports::Clock,
                Arc::new(queue_ports::Environment),
                Arc::new(NoAudit),
            )
            .await
            .unwrap();
        let actor = ActorContext {
            client_id: id(1),
            auth_generation: 12,
        };
        let original =
            ValidatedMutation::Submit(service.validate_body(b"before-actual-rollback").unwrap());
        let x = service.register(&actor, id(81), &original).await.unwrap();
        assert!(matches!(
            service.consume(&actor, id(81), x.id(), original).await,
            ConsumeReply::Resolved { .. }
        ));
        let edit = ValidatedMutation::Edit {
            item: x.id().entity(),
            revision: 1,
            body: service.validate_body(b"after-actual-rollback").unwrap(),
        };
        let r = service.register(&actor, id(81), &edit).await.unwrap();
        setup
            .repo
            .faults_for_test()
            .arm(point, Action::Fail)
            .unwrap();
        assert!(matches!(
            service.consume(&actor, id(81), r.id(), edit).await,
            ConsumeReply::Pending { .. }
        ));
        assert_eq!(setup.phase(r.id()).await.as_deref(), Some("applying"));
        let pending = service
            .inspect_queue(&actor, id(81), |read| match read {
                QueueRead::Processing { applied, queue, .. } => {
                    let entry = queue.waiting().next().unwrap();
                    (
                        applied,
                        entry.id(),
                        entry.revision(),
                        entry.input().body.body().clone(),
                    )
                }
                QueueRead::Stable { .. } => {
                    panic!("real SQL rollback must keep applied content explicitly tentative")
                }
            })
            .await
            .unwrap();
        assert_eq!(
            pending,
            (true, x.id().entity(), 2, b"after-actual-rollback".to_vec())
        );
        let fact = service.reconcile(id(81), r.id()).await.unwrap();
        assert!(
            matches!(fact,RequestFact::Settled {outcome:QueueOutcome::Edited {item,revision:2},..} if item==x.id().entity())
        );
        assert_eq!(
            service.reconcile(id(81), r.id()).await.err(),
            Some(RegistryError::NoOwnedOperation)
        );
        let version = service
            .inspect_queue(&actor, id(81), |read| match read {
                QueueRead::Stable { queue } => queue.waiting().next().unwrap().revision(),
                QueueRead::Processing { .. } => panic!("same receipt must be reliable after retry"),
            })
            .await
            .unwrap();
        assert_eq!(version, 2);
        assert_eq!(setup.count("node_request_operations").await, 2);
        assert!(service.shutdown(deadline()).await.joined);
        setup.close().await;
    }
}
