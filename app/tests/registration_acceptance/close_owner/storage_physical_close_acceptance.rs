//! REG-INTEG-CLOSE-001: actual Storage close, no deletion retry or scheduler sleep.
#[path = "physical_fixtures.rs"]
mod fixtures;
#[allow(
    dead_code,
    reason = "The copied trusted fixture also supplies controlled activity for other independent tests."
)]
mod queue_ports;
use super::test_hooks::{Action, Barrier, Operation, Point};
use crate::{
    application::{queue_owner::BlockedEnvironment, request_registry::*},
    domain::{
        ActorContext, EntityId,
        native::{NativeSessionKey, RuntimeEpoch},
    },
    storage::Storage,
};
use fixtures::{Area, connect, fixture_base};
use sqlx::Connection;
use std::{mem::ManuallyDrop, path::Path, sync::Arc};
use tokio::time::{Duration, timeout};
const fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
async fn seeded() -> (ManuallyDrop<Area>, Storage) {
    let area = ManuallyDrop::new(Area::new());
    fixture_base(&area, 3).await;
    let storage = Storage::open(&area.db()).await.unwrap();
    let mut c = connect(&area.db()).await;
    sqlx::query("INSERT INTO product_sources VALUES(?,'physical-close-source')")
        .bind(id(80).encode())
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,'registry-fault-synthetic',NULL)")
        .bind(id(81).encode())
        .bind(id(80).encode())
        .execute(&mut c)
        .await
        .unwrap();
    c.close().await.unwrap();
    (area, storage)
}
#[cfg(windows)]
fn exclusive(path: &Path) -> Result<(), std::io::Error> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
        .map(drop)
}
#[cfg(windows)]
#[tokio::test]
async fn reg_close_01_normal_repository_return_then_storage_close_has_no_sqlite_worker_handle() {
    let mut failures = Vec::new();
    for _ in 0..24 {
        let (area, storage) = seeded().await;
        let intent = RegisteredIntent::new(
            id(1),
            QueueAction::Submit,
            id(81),
            None,
            None,
            Some([5; 32]),
        )
        .unwrap();
        let current = CurrentRequestActor::new(
            ActorContext {
                client_id: id(1),
                auth_generation: 12,
            },
            id(81),
            NativeSessionKey::from_config(Some(id(80)), "registry-fault-synthetic").unwrap(),
            Some(QueueAction::Submit),
        );
        let at = OperationContext {
            runtime: RuntimeEpoch(id(90)),
            observed_at_ms: 0,
        };
        let deadline = RegistryDeadline::new(tokio::time::Instant::now() + Duration::from_secs(20));
        storage
            .request_registry()
            .register(
                &current,
                &intent,
                RegistrationId::new(id(100)),
                &at,
                &deadline,
            )
            .await
            .unwrap();
        storage.close().await;
        // No async work between close returning and the physical handle oracle.
        let result = exclusive(&area.db());
        if let Err(error) = result {
            failures.push((area.0.clone(), error.raw_os_error()));
        } else {
            drop(ManuallyDrop::into_inner(area));
        }
    }
    println!(
        "REG-INTEG-CLOSE-001 actual normal close attempts=24 failures={} first={:?}; failing owned DBs retained",
        failures.len(),
        failures.first()
    );
    assert!(
        failures.is_empty(),
        "Storage.close returned before its repository SQLite workers released handles"
    );
}
#[cfg(windows)]
#[tokio::test]
async fn reg_close_02_node_owned_begin_cancellation_joins_sql_worker_before_storage_close_returns()
{
    let mut failures = Vec::new();
    for after_commit in [false, true] {
        let (area, storage) = seeded().await;
        let (service, _) = storage
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
        let mutation = ValidatedMutation::Submit(
            service
                .validate_body(b"synthetic-owned-close-input")
                .unwrap(),
        );
        let r = service.register(&actor, id(81), &mutation).await.unwrap();
        let request = r.id();
        let barrier = Barrier::new();
        storage
            .request_registry()
            .faults_for_test()
            .arm(
                if after_commit {
                    Point::AfterCommit(Operation::Begin)
                } else {
                    Point::BeforeCommit(Operation::Begin)
                },
                Action::Pause(barrier.clone()),
            )
            .unwrap();
        let owned = service.clone();
        let mut waiter =
            tokio::spawn(async move { owned.consume(&actor, id(81), request, mutation).await });
        timeout(Duration::from_secs(20), barrier.arrived())
            .await
            .unwrap();
        timeout(Duration::from_secs(20), storage.close())
            .await
            .unwrap();
        let result = exclusive(&area.db());
        if let Err(error) = result {
            failures.push((area.0.clone(), after_commit, error.raw_os_error()));
        }
        barrier.release();
        if timeout(Duration::from_secs(20), &mut waiter).await.is_err() {
            waiter.abort();
            let _ = waiter.await;
            panic!("node-owned response waiter survived Storage.close");
        }
        let mut check = connect(&area.db()).await;
        let phase: Option<String> =
            sqlx::query_scalar("SELECT phase FROM node_request_operations WHERE request_id=?")
                .bind(request.encode())
                .fetch_optional(&mut check)
                .await
                .unwrap();
        assert_eq!(
            phase.as_deref(),
            if after_commit { Some("applying") } else { None }
        );
        check.close().await.unwrap();
        if !failures.iter().any(|(path, _, _)| path == &area.0) {
            drop(ManuallyDrop::into_inner(area));
        }
    }
    println!(
        "REG-INTEG-CLOSE-001 actual node-owned begin BI/BA failures={} details={failures:?}; failing owned DBs retained",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "Storage.close joined node tasks but not their detached SQLite workers"
    );
}

use super::connection_close_test_hooks::{CloseBarrier, ClosePoint};
use crate::session_storage::ConnectionCloseStatus;
fn grant(action: Option<QueueAction>) -> CurrentRequestActor {
    CurrentRequestActor::new(
        ActorContext {
            client_id: id(1),
            auth_generation: 12,
        },
        id(81),
        NativeSessionKey::from_config(Some(id(80)), "registry-fault-synthetic").unwrap(),
        action,
    )
}
const fn stamp() -> OperationContext {
    OperationContext {
        runtime: RuntimeEpoch(id(90)),
        observed_at_ms: -1,
    }
}
fn until() -> RegistryDeadline {
    RegistryDeadline::new(tokio::time::Instant::now() + Duration::from_secs(20))
}
fn submit_intent() -> RegisteredIntent {
    RegisteredIntent::new(
        id(1),
        QueueAction::Submit,
        id(81),
        None,
        None,
        Some([5; 32]),
    )
    .unwrap()
}
async fn pending_once<F: std::future::Future>(mut future: std::pin::Pin<&mut F>) {
    use std::{future::poll_fn, task::Poll};
    let observed = poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx))).await;
    assert!(
        matches!(observed, Poll::Pending),
        "expected an actually pending public Future, not an invented scheduling point"
    );
}

#[cfg(windows)]
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep the complete real transaction and close evidence adjacent for independent review."
)]
async fn reg_close_03_rollback_and_commit_unknown_close_without_rewriting_sql_facts() {
    for operation in [Operation::Register, Operation::Begin, Operation::Settle] {
        for committed in [false, true] {
            let (area, storage) = seeded().await;
            let repo = storage.request_registry();
            let intent = submit_intent();
            let request = RegistrationId::new(id(100));
            let current = grant(Some(QueueAction::Submit));
            let at = stamp();
            let limit = until();
            let mut lease = None;
            if !matches!(operation, Operation::Register) {
                repo.register(&current, &intent, request, &at, &limit)
                    .await
                    .unwrap();
            }
            if matches!(operation, Operation::Settle) {
                let probe = OperationProbe::new(request, id(101), &at, &current, intent.clone());
                let BeginRecord::Started(stored) = repo
                    .begin_consume(&current, request, &intent, id(101), &at, &limit)
                    .await
                    .unwrap()
                else {
                    panic!("first operation did not start")
                };
                lease = Some(ConsumeLease::mint(&probe, stored).unwrap());
            }
            repo.faults_for_test()
                .arm(
                    if committed {
                        Point::AfterCommit(operation)
                    } else {
                        Point::BeforeCommit(operation)
                    },
                    if committed {
                        Action::Unconfirmed
                    } else {
                        Action::Fail
                    },
                )
                .unwrap();
            let error = match operation {
                Operation::Register => repo
                    .register(&current, &intent, request, &at, &limit)
                    .await
                    .err(),
                Operation::Begin => repo
                    .begin_consume(&current, request, &intent, id(101), &at, &limit)
                    .await
                    .err(),
                Operation::Settle => repo
                    .settle(
                        lease.as_ref().unwrap(),
                        &QueueOutcome::Queued {
                            item: request.entity(),
                            revision: 1,
                        },
                        &at,
                        &limit,
                    )
                    .await
                    .err(),
                _ => unreachable!("fixture enumerates only transaction kinds"),
            };
            assert_eq!(
                error,
                Some(if committed {
                    RegistryError::CommitUnconfirmed
                } else {
                    RegistryError::StorageUnavailable
                })
            );
            storage.close().await;
            exclusive(&area.db()).unwrap();
            assert!(matches!(
                storage.registry_close_status(),
                ConnectionCloseStatus::Complete
            ));
            let mut c = connect(&area.db()).await;
            let registrations: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM node_request_registrations")
                    .fetch_one(&mut c)
                    .await
                    .unwrap();
            let phase: Option<String> =
                sqlx::query_scalar("SELECT phase FROM node_request_operations WHERE request_id=?")
                    .bind(request.encode())
                    .fetch_optional(&mut c)
                    .await
                    .unwrap();
            assert_eq!(
                registrations,
                i64::from(!matches!(operation, Operation::Register) || committed)
            );
            assert_eq!(
                phase.as_deref(),
                match operation {
                    Operation::Settle => Some(if committed { "settled" } else { "applying" }),
                    Operation::Begin if committed => Some("applying"),
                    _ => None,
                }
            );
            c.close().await.unwrap();
            drop(ManuallyDrop::into_inner(area));
        }
    }
}

#[cfg(windows)]
#[tokio::test]
async fn reg_close_04_before_close_waiters_can_drop_without_losing_the_owned_close() {
    close_waiter_case(ClosePoint::BeforeClose).await;
}
#[cfg(windows)]
#[tokio::test]
async fn reg_close_05_after_ack_still_requires_join_and_no_five_second_escape() {
    close_waiter_case(ClosePoint::AfterCloseAck).await;
}
#[cfg(windows)]
async fn close_waiter_case(point: ClosePoint) {
    let (area, storage) = seeded().await;
    let storage = Arc::new(storage);
    let repo = storage.request_registry();
    let gate = CloseBarrier::new();
    repo.connection_closer_for_test()
        .pause_next(point, gate.clone())
        .unwrap();
    let request = RegistrationId::new(id(100));
    let io = tokio::spawn(async move {
        repo.register(
            &grant(Some(QueueAction::Submit)),
            &submit_intent(),
            request,
            &stamp(),
            &until(),
        )
        .await
    });
    timeout(Duration::from_secs(20), gate.arrived())
        .await
        .unwrap();
    let mut first = Box::pin(storage.close());
    pending_once(first.as_mut()).await;
    assert!(matches!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Pending
    ));
    io.abort();
    assert!(io.await.unwrap_err().is_cancelled());
    drop(first);
    let mut second = Box::pin(storage.close());
    pending_once(second.as_mut()).await;
    let mut third = Box::pin(storage.close());
    pending_once(third.as_mut()).await;
    let held = timeout(Duration::from_secs(6), second.as_mut())
        .await
        .is_err();
    let still_pending = matches!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Pending
    );
    gate.release();
    assert!(
        held && still_pending,
        "a five-second timeout or cancelled waiter lost the close ownership"
    );
    timeout(Duration::from_secs(20), second).await.unwrap();
    timeout(Duration::from_secs(20), third).await.unwrap();
    exclusive(&area.db()).unwrap();
    assert!(matches!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Complete
    ));
    storage.close().await;
    exclusive(&area.db()).unwrap();
    let mut c = connect(&area.db()).await;
    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM node_request_registrations WHERE request_id=?")
            .bind(request.encode())
            .fetch_one(&mut c)
            .await
            .unwrap();
    assert_eq!(
        rows, 1,
        "actual COMMIT must survive cancellation of its response and close waiters"
    );
    c.close().await.unwrap();
    drop(ManuallyDrop::into_inner(area));
}

#[cfg(windows)]
#[tokio::test]
async fn reg_close_06_lease_before_acquire_is_counted_and_seal_refuses_new_sql() {
    let (area, storage) = seeded().await;
    let repo = storage.request_registry();
    let blocker = repo.pool.acquire().await.unwrap();
    let current = grant(Some(QueueAction::Submit));
    let intent = submit_intent();
    let at = stamp();
    let limit = until();
    let mut registration =
        Box::pin(repo.register(&current, &intent, RegistrationId::new(id(100)), &at, &limit));
    pending_once(registration.as_mut()).await;
    let mut closing = Box::pin(storage.close());
    pending_once(closing.as_mut()).await;
    assert!(
        matches!(
            storage.registry_close_status(),
            ConnectionCloseStatus::Pending
        ),
        "an in-flight lease disappeared before pool acquire finished"
    );
    drop(blocker);
    // The first deadline records a caller cancellation; NodeStopping must be
    // observable through an independent deadline once connections are sealed.
    let sealed_deadline = until();
    assert_eq!(
        repo.register(
            &current,
            &intent,
            RegistrationId::new(id(101)),
            &at,
            &sealed_deadline
        )
        .await
        .err(),
        Some(RegistryError::NodeStopping)
    );
    drop(registration);
    timeout(Duration::from_secs(20), closing).await.unwrap();
    exclusive(&area.db()).unwrap();
    assert!(matches!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Complete
    ));
    let mut c = connect(&area.db()).await;
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_request_registrations")
        .fetch_one(&mut c)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    c.close().await.unwrap();
    drop(ManuallyDrop::into_inner(area));
}

mod real_sql_authority;
#[cfg(windows)]
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "The SQL resolver and event ordering must remain visible at the actual blocked transaction."
)]
async fn reg_close_07_same_max1_sql_authority_reads_and_failure_events_progress_during_begin() {
    use crate::application::queue_owner::{ConsumeReply, QueueOwnerEvent, QueueRead};
    let (area, storage) = seeded().await;
    let mut c = connect(&area.db()).await;
    sqlx::query("INSERT INTO product_targets VALUES(?,?,'other-native',NULL)")
        .bind(id(82).encode())
        .bind(id(80).encode())
        .execute(&mut c)
        .await
        .unwrap();
    c.close().await.unwrap();
    let actual_pool = storage.request_registry().pool;
    let (service, driver) = storage
        .start_queue_service_with_audit_for_fixture(
            real_sql_authority::RealSqlAuthority { pool: actual_pool },
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
        let mutation = ValidatedMutation::Submit(service.validate_body(body).unwrap());
        let r = service.register(&actor, id(81), &mutation).await.unwrap();
        ids.push(r.id().entity());
        assert!(matches!(
            service.consume(&actor, id(81), r.id(), mutation).await,
            ConsumeReply::Resolved { .. }
        ));
    }
    let work = driver
        .open_external_work(id(81), queue_ports::resource())
        .await
        .unwrap();
    let mutation = ValidatedMutation::Submit(service.validate_body(b"new-D").unwrap());
    let r = service.register(&actor, id(81), &mutation).await.unwrap();
    let request = r.id();
    let gate = Barrier::new();
    storage
        .request_registry()
        .faults_for_test()
        .arm(
            Point::BeforeCommit(Operation::Begin),
            Action::Pause(gate.clone()),
        )
        .unwrap();
    let owned = service.clone();
    let task = tokio::spawn(async move { owned.consume(&actor, id(81), request, mutation).await });
    timeout(Duration::from_secs(20), gate.arrived())
        .await
        .unwrap();
    let event = timeout(
        Duration::from_secs(10),
        driver.observe(id(81), QueueOwnerEvent::ExternalForegroundFailure { work }),
    )
    .await;
    let queue = timeout(
        Duration::from_secs(10),
        service.inspect_queue(&actor, id(81), |read| match read {
            QueueRead::Processing { applied, queue, .. } => (
                !applied,
                queue
                    .waiting()
                    .map(crate::domain::input_queue::QueueEntry::suspended)
                    .collect::<Vec<_>>(),
            ),
            QueueRead::Stable { .. } => (false, Vec::new()),
        }),
    )
    .await;
    let other = timeout(
        Duration::from_secs(10),
        service.inspect_queue(&actor, id(82), |read| match read {
            QueueRead::Stable { queue } => queue.waiting().len() == 0,
            QueueRead::Processing { .. } => false,
        }),
    )
    .await;
    gate.release();
    assert!(
        matches!(event, Ok(Ok(()))),
        "real SQL target resolver was blocked behind the registry's connection ownership"
    );
    assert_eq!(queue.unwrap().unwrap(), (true, vec![true, true]));
    assert!(
        other.unwrap().unwrap(),
        "unrelated queue read required the registry write to finish"
    );
    assert!(matches!(
        timeout(Duration::from_secs(20), task)
            .await
            .unwrap()
            .unwrap(),
        ConsumeReply::Resolved { .. }
    ));
    let snapshot = service
        .inspect_queue(&actor, id(81), |read| match read {
            QueueRead::Stable { queue } => queue
                .waiting()
                .map(|entry| (entry.id(), entry.suspended()))
                .collect::<Vec<_>>(),
            QueueRead::Processing { .. } => panic!("management result remained tentative"),
        })
        .await
        .unwrap();
    assert_eq!(
        snapshot,
        vec![(ids[0], true), (ids[1], true), (request.entity(), false)]
    );
    storage.close().await;
    exclusive(&area.db()).unwrap();
    assert!(matches!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Complete
    ));
    drop(ManuallyDrop::into_inner(area));
}

#[cfg(windows)]
#[tokio::test]
async fn reg_close_08_after_ack_report_error_preserves_register_applying_and_settled_facts() {
    for operation in [Operation::Register, Operation::Begin, Operation::Settle] {
        let (area, storage) = seeded().await;
        let repo = storage.request_registry();
        let current = grant(Some(QueueAction::Submit));
        let intent = submit_intent();
        let at = stamp();
        let limit = until();
        let request = RegistrationId::new(id(100));
        let mut lease = None;
        if !matches!(operation, Operation::Register) {
            repo.register(&current, &intent, request, &at, &limit)
                .await
                .unwrap();
        }
        if matches!(operation, Operation::Settle) {
            let probe = OperationProbe::new(request, id(101), &at, &current, intent.clone());
            let BeginRecord::Started(stored) = repo
                .begin_consume(&current, request, &intent, id(101), &at, &limit)
                .await
                .unwrap()
            else {
                panic!("begin did not acquire its actual Applying fact")
            };
            lease = Some(ConsumeLease::mint(&probe, stored).unwrap());
        }
        repo.connection_closer_for_test()
            .fail_next_result_after_ack()
            .unwrap();
        match operation {
            Operation::Register => assert_eq!(
                repo.register(&current, &intent, request, &at, &limit)
                    .await
                    .unwrap()
                    .id(),
                request
            ),
            Operation::Begin => assert!(matches!(
                repo.begin_consume(&current, request, &intent, id(101), &at, &limit)
                    .await
                    .unwrap(),
                BeginRecord::Started(_)
            )),
            Operation::Settle => assert!(matches!(
                repo.settle(
                    lease.as_ref().unwrap(),
                    &QueueOutcome::Queued {
                        item: request.entity(),
                        revision: 1
                    },
                    &at,
                    &limit
                )
                .await
                .unwrap(),
                RequestFact::Settled { .. }
            )),
            _ => unreachable!("enumerated fixture operations"),
        }
        storage.close().await;
        exclusive(&area.db()).unwrap();
        assert!(matches!(
            storage.registry_close_status(),
            ConnectionCloseStatus::Unconfirmed
        ));
        storage.close().await;
        assert!(
            matches!(
                storage.registry_close_status(),
                ConnectionCloseStatus::Unconfirmed
            ),
            "repeating close erased a saved unconfirmed report"
        );
        let mut c = connect(&area.db()).await;
        let rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM node_request_registrations WHERE request_id=?",
        )
        .bind(request.encode())
        .fetch_one(&mut c)
        .await
        .unwrap();
        let phase: Option<String> =
            sqlx::query_scalar("SELECT phase FROM node_request_operations WHERE request_id=?")
                .bind(request.encode())
                .fetch_optional(&mut c)
                .await
                .unwrap();
        assert_eq!(rows, 1);
        assert_eq!(
            phase.as_deref(),
            match operation {
                Operation::Register => None,
                Operation::Begin => Some("applying"),
                Operation::Settle => Some("settled"),
                _ => unreachable!(),
            }
        );
        c.close().await.unwrap();
        drop(ManuallyDrop::into_inner(area));
    }
}

mod close_report_repository;
#[cfg(windows)]
#[tokio::test]
async fn reg_close_09_actual_owner_close_report_loss_cannot_apply_input_twice() {
    use crate::application::queue_owner::{ConsumeReply, QueueRead};
    use close_report_repository::{CloseReportRepository, FailAt};
    use std::sync::atomic::AtomicBool;
    for point in [FailAt::Begin, FailAt::Settle] {
        let (area, storage) = seeded().await;
        let (service, _) = storage
            .start_queue_service_with_repository_and_audit_for_fixture(
                queue_ports::Authority,
                queue_ports::Validator,
                queue_ports::Clock,
                Arc::new(BlockedEnvironment),
                Arc::new(NoAudit),
                move |inner| {
                    Ok(CloseReportRepository {
                        inner,
                        point,
                        armed: Arc::new(AtomicBool::new(false)),
                    })
                },
            )
            .await
            .unwrap();
        let actor = ActorContext {
            client_id: id(1),
            auth_generation: 12,
        };
        let mutation = ValidatedMutation::Submit(
            service
                .validate_body(b"ack-report-loss-synthetic-body")
                .unwrap(),
        );
        let r = service.register(&actor, id(81), &mutation).await.unwrap();
        let first = service.consume(&actor, id(81), r.id(), mutation).await;
        let ConsumeReply::Resolved { fact, returned } = first else {
            panic!("close report loss changed a committed SQL outcome")
        };
        assert!(returned.is_none());
        assert!(
            matches!(fact,RequestFact::Settled {outcome:QueueOutcome::Queued {item,revision:1},..} if item==r.id().entity())
        );
        let retry = ValidatedMutation::Submit(
            service
                .validate_body(b"ack-report-loss-synthetic-body")
                .unwrap(),
        );
        let ConsumeReply::Resolved {
            fact: repeated,
            returned,
        } = service.consume(&actor, id(81), r.id(), retry).await
        else {
            panic!("retry failed to return its original fact")
        };
        assert_eq!(repeated, fact);
        assert!(returned.is_some());
        let ids = service
            .inspect_queue(&actor, id(81), |read| match read {
                QueueRead::Stable { queue } => queue
                    .waiting()
                    .map(|entry| (entry.id(), entry.revision()))
                    .collect::<Vec<_>>(),
                QueueRead::Processing { .. } => {
                    panic!("close result report was mistaken for uncommitted necessary SQL")
                }
            })
            .await
            .unwrap();
        assert_eq!(ids, vec![(r.id().entity(), 1)]);
        storage.close().await;
        exclusive(&area.db()).unwrap();
        assert!(matches!(
            storage.registry_close_status(),
            ConnectionCloseStatus::Unconfirmed
        ));
        drop(ManuallyDrop::into_inner(area));
    }
}
