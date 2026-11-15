//! Independent P-layer consumer cases. Real SQLite and the actual queue owner;
//! authority, body validator, clock, activity and audit are declared trusted test ports.
#![allow(
    clippy::many_single_char_names,
    reason = "X/Y/W/D identifiers match the frozen event-order examples."
)]
mod support;
use acp_remote::{
    application::{
        queue_owner::{ConsumeReply, DriverError, QueueOwnerEvent, StartupStatus},
        request_registry::{
            AuditFailure, QueueOutcome, QueueRejection, RegistrationId, RegistryError, RequestFact,
            ValidatedMutation,
        },
    },
    domain::input_queue::{DispatchReadiness, QueueDisposition, QueueError},
};
use sqlx::Connection;
use std::sync::atomic::Ordering;
use support::*;
use tokio::time::timeout;

async fn apply(service: &Service, bytes: &[u8]) -> RegistrationId {
    let registered = registered(service, &A, T1, bytes).await;
    let (fact, returned) = resolved(
        service
            .consume(&A, T1, registered.id(), submit(service, bytes))
            .await,
    );
    assert!(returned.is_none());
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Queued {
            item: registered.id().entity(),
            revision: 1
        }
    );
    registered.id()
}
fn expect_rejected(reply: ConsumeReply<Body>, expected: RegistryError) {
    match reply {
        ConsumeReply::Refused { error, returned } => {
            assert_eq!(error, expected);
            drop(returned);
        }
        _ => panic!("expected registry refusal"),
    }
}
fn waiting_ids(snapshot: &View) -> Vec<acp_remote::domain::EntityId> {
    snapshot.waiting.iter().map(|e| e.id).collect()
}

#[tokio::test]
async fn cl104_reg01_registration_only_then_actual_queue_once_no_old_run_path() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let registration = registered(&service, &A, T1, b"X/body-canary").await;
    assert_eq!(fixture.sql_count("node_request_registrations").await, 1);
    assert_eq!(fixture.sql_count("node_request_operations").await, 0);
    assert!(view(&service, &A, T1).await.waiting.is_empty());
    assert!(matches!(
        service
            .query_request(&A, T1, registration.id())
            .await
            .unwrap(),
        RequestFact::Registered(_)
    ));
    let (fact, returned) = resolved(
        service
            .consume(
                &A,
                T1,
                registration.id(),
                submit(&service, b"X/body-canary"),
            )
            .await,
    );
    assert!(returned.is_none());
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Queued {
            item: registration.id().entity(),
            revision: 1
        }
    );
    let snapshot = view(&service, &A, T1).await;
    assert!(!snapshot.processing);
    assert_eq!(snapshot.waiting.len(), 1);
    assert_eq!(snapshot.waiting[0].bytes, b"X/body-canary");
    assert_eq!(fixture.sql_count("product_commands").await, 0);
    assert_eq!(fixture.sql_count("product_runs").await, 0);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl017_reg01_same_registration_retries_return_facts_without_second_apply() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let request = apply(&service, b"same content").await;
    let before = view(&service, &A, T1).await;
    for _ in 0..3 {
        let (fact, returned) = resolved(
            service
                .consume(&A, T1, request, submit(&service, b"same content"))
                .await,
        );
        assert_eq!(
            outcome(&fact),
            QueueOutcome::Queued {
                item: request.entity(),
                revision: 1
            }
        );
        assert!(matches!(returned, Some(ValidatedMutation::Submit(_))));
    }
    assert_eq!(view(&service, &A, T1).await, before);
    assert_eq!(fixture.sql_count("node_request_operations").await, 1);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl018_reg01_changed_intent_and_unknown_id_have_no_fallback_insert() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let registration = registered(&service, &A, T1, b"bound-v1").await;
    expect_rejected(
        service
            .consume(&A, T1, registration.id(), submit(&service, b"other-v2"))
            .await,
        RegistryError::Conflict,
    );
    expect_rejected(
        service
            .consume(&A, T1, registration.id(), ValidatedMutation::Resume)
            .await,
        RegistryError::Conflict,
    );
    let response = service
        .consume(&B, T1, registration.id(), submit(&service, b"bound-v1"))
        .await;
    match response {
        ConsumeReply::Refused { error, .. } => assert!(matches!(
            error,
            RegistryError::InvalidActor | RegistryError::Conflict
        )),
        _ => panic!("foreign actor consumed A's registration"),
    }
    expect_rejected(
        service
            .consume(
                &A,
                T1,
                RegistrationId::new(id(98)),
                submit(&service, b"bound-v1"),
            )
            .await,
        RegistryError::UnknownRegistration,
    );
    assert_eq!(fixture.sql_count("node_request_registrations").await, 1);
    assert_eq!(fixture.sql_count("node_request_operations").await, 0);
    assert!(view(&service, &A, T1).await.waiting.is_empty());
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl018_reg02_cross_client_edit_identity_and_retry_version_are_independent() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let x = apply(&service, b"X-v1").await;
    let mutation = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 1,
        body: service.validate_body(b"X-v2").unwrap(),
    };
    let edit = service.register(&B, T1, &mutation).await.unwrap();
    let (fact, returned) = resolved(service.consume(&B, T1, edit.id(), mutation).await);
    assert!(returned.is_none());
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Edited {
            item: x.entity(),
            revision: 2
        }
    );
    let duplicate = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 1,
        body: service.validate_body(b"X-v2").unwrap(),
    };
    let (fact, returned) = resolved(service.consume(&B, T1, edit.id(), duplicate).await);
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Edited {
            item: x.entity(),
            revision: 2
        }
    );
    assert!(returned.is_some());
    let snapshot = view(&service, &A, T1).await;
    assert_eq!(snapshot.waiting.len(), 1);
    assert_eq!(snapshot.waiting[0].revision, 2);
    assert_eq!(snapshot.waiting[0].submitter, A.id);
    assert_eq!(snapshot.waiting[0].editor, B.id);
    assert_eq!(snapshot.waiting[0].bytes, b"X-v2");
    expect_rejected(
        service.consume(&A, T1, x, submit(&service, b"X-v2")).await,
        RegistryError::Conflict,
    );
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl017_reg03_full_is_a_stable_rejection_and_new_identity_is_a_new_attempt() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let x = apply(&service, b"X").await;
    apply(&service, b"Y").await;
    apply(&service, b"Z").await;
    let d = registered(&service, &A, T1, b"D").await;
    let (fact, returned) = resolved(
        service
            .consume(&A, T1, d.id(), submit(&service, b"D"))
            .await,
    );
    assert_eq!(outcome(&fact), QueueOutcome::Rejected(QueueRejection::Full));
    assert!(returned.is_some());
    let cancel = ValidatedMutation::Cancel {
        item: x.entity(),
        revision: 1,
    };
    let c = service.register(&B, T1, &cancel).await.unwrap();
    resolved(service.consume(&B, T1, c.id(), cancel).await);
    let (fact, _) = resolved(
        service
            .consume(&A, T1, d.id(), submit(&service, b"D"))
            .await,
    );
    assert_eq!(outcome(&fact), QueueOutcome::Rejected(QueueRejection::Full));
    assert_eq!(view(&service, &A, T1).await.waiting.len(), 2);
    let fresh = apply(&service, b"D").await;
    assert_ne!(fresh, d.id());
    assert_eq!(view(&service, &A, T1).await.waiting.len(), 3);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl075_reg02_real_begin_commit_lost_response_recovers_once_without_actor_recheck() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let r = registered(&service, &A, T1, b"X").await;
    fixture
        .control
        .arm(Point::BeginAfter, Action::ReturnUnconfirmed);
    let reply = service
        .consume(&A, T1, r.id(), submit(&service, b"X"))
        .await;
    assert!(matches!(reply,ConsumeReply::Pending{request} if request==r.id()));
    assert_eq!(fixture.phase(r.id()).await.as_deref(), Some("applying"));
    let snapshot = view(&service, &A, T1).await;
    assert!(snapshot.processing);
    assert!(!snapshot.applied);
    assert!(snapshot.waiting.is_empty());
    fixture.revoke(A).await;
    let fact = service.reconcile(T1, r.id()).await.unwrap();
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Queued {
            item: r.id().entity(),
            revision: 1
        }
    );
    assert_eq!(view(&service, &B, T1).await.waiting.len(), 1);
    assert_eq!(fixture.control.begin_calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        service.query_request(&A, T1, r.id()).await,
        Err(RegistryError::InvalidActor)
    ));
    assert!(matches!(
        service.reconcile(T1, r.id()).await,
        Err(RegistryError::NoOwnedOperation)
    ));
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl076_reg01_real_settle_commit_lost_response_does_not_edit_twice() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let x = apply(&service, b"X1").await;
    let edit = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 1,
        body: service.validate_body(b"X2").unwrap(),
    };
    let reg = service.register(&B, T1, &edit).await.unwrap();
    fixture
        .control
        .arm(Point::SettleAfter, Action::ReturnUnconfirmed);
    let reply = service.consume(&B, T1, reg.id(), edit).await;
    assert!(matches!(reply,ConsumeReply::Pending{request} if request==reg.id()));
    assert_eq!(fixture.phase(reg.id()).await.as_deref(), Some("settled"));
    let snapshot = view(&service, &A, T1).await;
    assert!(snapshot.processing && snapshot.applied);
    assert_eq!(snapshot.waiting[0].revision, 2);
    assert_eq!(snapshot.waiting[0].bytes, b"X2");
    let fact = service.reconcile(T1, reg.id()).await.unwrap();
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Edited {
            item: x.entity(),
            revision: 2
        }
    );
    assert!(!view(&service, &A, T1).await.processing);
    assert_eq!(view(&service, &A, T1).await.waiting[0].revision, 2);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl076_reg02_repository_unavailable_before_settle_retries_only_known_outcome() {
    // This is an unavailable repository boundary, not evidence of SQL rollback.
    // The actual pre-COMMIT rollback cases are independently implemented in F.
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let x = apply(&service, b"X1").await;
    let edit = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 1,
        body: service.validate_body(b"X2").unwrap(),
    };
    let reg = service.register(&B, T1, &edit).await.unwrap();
    fixture
        .control
        .arm(Point::SettleBefore, Action::ReturnUnavailable);
    let reply = service.consume(&B, T1, reg.id(), edit).await;
    assert!(matches!(reply, ConsumeReply::Pending { .. }));
    assert_eq!(fixture.phase(reg.id()).await.as_deref(), Some("applying"));
    let snapshot = view(&service, &A, T1).await;
    assert!(snapshot.processing && snapshot.applied);
    assert_eq!(snapshot.waiting[0].revision, 2);
    service.reconcile(T1, reg.id()).await.unwrap();
    assert_eq!(view(&service, &A, T1).await.waiting[0].revision, 2);
    let relevant: Vec<_> = fixture
        .control
        .settle_values
        .lock()
        .unwrap()
        .iter()
        .filter(|(request, _)| *request == reg.id())
        .map(|(_, outcome)| *outcome)
        .collect();
    assert_eq!(
        relevant,
        vec![
            QueueOutcome::Edited {
                item: x.entity(),
                revision: 2
            };
            2
        ]
    );
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl092_reg01_registered_but_not_admitted_actor_is_rechecked_in_real_sql() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let r = registered(&service, &A, T1, b"X").await;
    fixture.revoke(A).await;
    expect_rejected(
        service
            .consume(&A, T1, r.id(), submit(&service, b"X"))
            .await,
        RegistryError::InvalidActor,
    );
    assert_eq!(fixture.phase(r.id()).await, None);
    assert!(view(&service, &B, T1).await.waiting.is_empty());
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl092_reg02_caller_abort_after_real_commit_does_not_drop_node_owned_operation() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let r = registered(&service, &A, T1, b"node-owned").await;
    let gate = Gate::new();
    fixture
        .control
        .arm(Point::BeginAfter, Action::Pause(gate.clone()));
    let task_service = service.clone();
    let request_id = r.id();
    let task = tokio::spawn(async move {
        let mutation = submit(&task_service, b"node-owned");
        task_service.consume(&A, T1, request_id, mutation).await
    });
    gate.wait().await;
    assert_eq!(fixture.phase(r.id()).await.as_deref(), Some("applying"));
    task.abort();
    let _ = task.await;
    gate.release();
    timeout(deadline(), async {
        loop {
            let snapshot = view(&service, &B, T1).await;
            if !snapshot.processing && snapshot.waiting.len() == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fixture.sql_count("node_request_operations").await, 1);
    assert_eq!(fixture.control.begin_calls.load(Ordering::SeqCst), 1);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl099_reg01_processing_projection_reports_applied_body_as_tentative() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    let r = registered(&service, &A, T1, b"new-body").await;
    let gate = Gate::new();
    fixture
        .control
        .arm(Point::SettleBefore, Action::Pause(gate.clone()));
    let task_service = service.clone();
    let request_id = r.id();
    let task = tokio::spawn(async move {
        let mutation = submit(&task_service, b"new-body");
        task_service.consume(&A, T1, request_id, mutation).await
    });
    gate.wait().await;
    let snapshot = view(&service, &B, T1).await;
    assert!(snapshot.processing && snapshot.applied);
    assert_eq!(snapshot.waiting[0].bytes, b"new-body");
    assert_eq!(fixture.phase(r.id()).await.as_deref(), Some("applying"));
    assert!(driver.prepare(T1).await.is_err());
    gate.release();
    resolved(task.await.unwrap());
    assert!(!view(&service, &B, T1).await.processing);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl087_reg01_service_clones_share_three_slots_and_other_target_can_progress() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    apply(&service, b"X").await;
    apply(&service, b"Y").await;
    let r1 = registered(&service, &A, T1, b"Z").await;
    let r2 = registered(&service, &B, T1, b"E").await;
    let c1 = service.clone();
    let c2 = service.clone();
    let (a, b) = tokio::join!(
        async {
            let mutation = submit(&c1, b"Z");
            c1.consume(&A, T1, r1.id(), mutation).await
        },
        async {
            let mutation = submit(&c2, b"E");
            c2.consume(&B, T1, r2.id(), mutation).await
        }
    );
    let outcomes = [outcome(&resolved(a).0), outcome(&resolved(b).0)];
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, QueueOutcome::Queued { .. }))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| **o == QueueOutcome::Rejected(QueueRejection::Full))
            .count(),
        1
    );
    assert_eq!(view(&service, &A, T1).await.waiting.len(), 3);
    let r = registered(&service, &B, T2, b"other target").await;
    let (fact, _) = resolved(
        service
            .consume(&B, T2, r.id(), submit(&service, b"other target"))
            .await,
    );
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Queued {
            item: r.id().entity(),
            revision: 1
        }
    );
    assert_eq!(view(&service, &B, T2).await.waiting.len(), 1);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl079_reg01_signed_node_times_do_not_create_expiry_or_change_queue_order() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let x = apply(&service, b"X").await;
    fixture.clock.0.store(-1, Ordering::SeqCst);
    let y = apply(&service, b"Y").await;
    fixture.clock.0.store(86_401_001, Ordering::SeqCst);
    let z = apply(&service, b"Z").await;
    let (fact, _) = resolved(service.consume(&A, T1, x, submit(&service, b"X")).await);
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Queued {
            item: x.entity(),
            revision: 1
        }
    );
    assert_eq!(
        waiting_ids(&view(&service, &A, T1).await),
        vec![x.entity(), y.entity(), z.entity()]
    );
    let mut sql = connection(&fixture.area.db()).await;
    let floor: i64 = sqlx::query_scalar("SELECT time_floor_ms FROM product_control")
        .fetch_one(&mut sql)
        .await
        .unwrap();
    assert_eq!(floor, 0);
    let ytime: i64 = sqlx::query_scalar(
        "SELECT registered_at_ms FROM node_request_registrations WHERE request_id=?",
    )
    .bind(y.encode())
    .fetch_one(&mut sql)
    .await
    .unwrap();
    assert_eq!(ytime, -1);
    sql.close().await.unwrap();
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl094_reg01_best_effort_audit_failures_are_visible_without_ledger_fault() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    for (bytes, failure) in [
        (b"X".as_slice(), AuditFailure::Unavailable),
        (b"Y".as_slice(), AuditFailure::Full),
    ] {
        *fixture.audit.mode.lock().unwrap() = AuditMode::Fail(failure);
        let request = apply(&service, bytes).await;
        timeout(deadline(), async {
            loop {
                let warnings = service.take_audit_warnings();
                if warnings
                    .iter()
                    .any(|w| w.request == request && w.failure == failure)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(fixture.phase(request).await.as_deref(), Some("settled"));
    }
    assert_eq!(view(&service, &B, T1).await.waiting.len(), 2);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl083_reg01_audit_wait_does_not_hold_pending_and_shutdown_joins_it() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let gate = Gate::new();
    *fixture.audit.mode.lock().unwrap() = AuditMode::Pause(gate.clone());
    apply(&service, b"X").await;
    gate.wait().await;
    assert!(!view(&service, &B, T1).await.processing);
    *fixture.audit.mode.lock().unwrap() = AuditMode::Pass;
    apply(&service, b"Y").await;
    assert_eq!(view(&service, &B, T1).await.waiting.len(), 2);
    let report = service.shutdown(closing_deadline()).await;
    assert!(report.joined);
    assert!(report.audit_cancelled >= 1);
    assert!(report.unresolved.is_empty());
    fixture.storage.close().await;
}

#[tokio::test]
async fn cl087_reg02_startup_commit_unknown_retains_handle_and_same_storage_claim() {
    let fixture = Fixture::new().await;
    fixture
        .control
        .arm(Point::StartupAfter, Action::ReturnUnconfirmed);
    let (service, _) = fixture.start().await;
    assert_eq!(service.startup_status(), StartupStatus::Unconfirmed);
    assert!(matches!(
        service
            .register(&A, T1, &submit(&service, b"blocked startup"))
            .await,
        Err(RegistryError::NodeStarting)
    ));
    assert!(matches!(
        fixture
            .storage
            .start_queue_service_with_audit_for_fixture(
                fixture.authority.clone(),
                Validator,
                fixture.clock.clone(),
                fixture.environment.clone(),
                fixture.audit.clone()
            )
            .await,
        Err(RegistryError::RuntimeAlreadyStarted)
    ));
    assert_eq!(
        service.reconcile_startup().await.unwrap(),
        StartupStatus::Running
    );
    apply(&service, b"after startup").await;
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl099_reg02_prepared_cancel_and_active_edit_cancel_obey_model_handoff() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    let x = apply(&service, b"X").await;
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
    let preparation = driver.prepare(T1).await.unwrap().unwrap();
    let cancel = ValidatedMutation::Cancel {
        item: x.entity(),
        revision: 1,
    };
    let r = service.register(&B, T1, &cancel).await.unwrap();
    assert_eq!(
        outcome(&resolved(service.consume(&B, T1, r.id(), cancel).await).0),
        QueueOutcome::Cancelled {
            item: x.entity(),
            revision: 1
        }
    );
    assert!(matches!(
        driver.begin_selection(&preparation).await,
        Err(DriverError::Queue(QueueError::StaleToken))
    ));
    let y = apply(&service, b"Y").await;
    let preparation = driver.prepare(T1).await.unwrap().unwrap();
    let active = driver.begin_selection(&preparation).await.unwrap();
    assert_eq!(active.item(), y.entity());
    let cancel = ValidatedMutation::Cancel {
        item: y.entity(),
        revision: 1,
    };
    let r = service.register(&B, T1, &cancel).await.unwrap();
    assert_eq!(
        outcome(&resolved(service.consume(&B, T1, r.id(), cancel).await).0),
        QueueOutcome::Rejected(QueueRejection::DispatchStarted)
    );
    assert_eq!(
        driver.active_key(T1).await.unwrap().unwrap().item(),
        y.entity()
    );
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl099_reg03_driver_rechecks_current_readiness_and_resource() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    apply(&service, b"X").await;
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
    let preparation = driver.prepare(T1).await.unwrap().unwrap();
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Blocked);
    assert!(matches!(
        driver.begin_selection(&preparation).await,
        Err(DriverError::Queue(QueueError::Blocked))
    ));
    fixture.environment.set_resource(&native(1), resource(104));
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Ready(resource(104)));
    assert!(matches!(
        driver.begin_selection(&preparation).await,
        Err(DriverError::Queue(QueueError::ResourceChanged))
    ));
    assert!(view(&service, &B, T1).await.active.is_none());
    assert_eq!(view(&service, &B, T1).await.waiting.len(), 1);
    let before = view(&service, &B, T1).await;
    driver.release(&preparation).await.unwrap();
    assert_eq!(view(&service, &B, T1).await, before);
    assert!(matches!(
        driver.begin_selection(&preparation).await,
        Err(DriverError::Queue(QueueError::StaleToken))
    ));
    let fresh = driver.prepare(T1).await.unwrap().unwrap();
    driver.release(&fresh).await.unwrap();
    assert_eq!(view(&service, &B, T1).await, before);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl110_reg01_ext01_failure_before_apply_only_suspends_old_waiting_items() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    let x = apply(&service, b"X").await;
    let y = apply(&service, b"Y").await;
    let work = driver.open_external_work(T1, resource(103)).await.unwrap();
    let r = registered(&service, &A, T1, b"new-D").await;
    let gate = Gate::new();
    fixture
        .control
        .arm(Point::BeginAfter, Action::Pause(gate.clone()));
    let task_service = service.clone();
    let request_id = r.id();
    let task = tokio::spawn(async move {
        let mutation = submit(&task_service, b"new-D");
        task_service.consume(&A, T1, request_id, mutation).await
    });
    gate.wait().await;
    assert_eq!(fixture.phase(r.id()).await.as_deref(), Some("applying"));
    driver
        .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work })
        .await
        .unwrap();
    let before = view(&service, &B, T1).await;
    assert!(before.processing && !before.applied);
    assert!(before.waiting.iter().all(|e| e.suspended));
    gate.release();
    resolved(task.await.unwrap());
    let after = view(&service, &B, T1).await;
    assert_eq!(
        waiting_ids(&after),
        vec![x.entity(), y.entity(), r.id().entity()]
    );
    assert!(after.waiting[0].suspended && after.waiting[1].suspended);
    assert!(!after.waiting[2].suspended);
    assert!(after.failed);
    assert!(after.active.is_none());
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl111_reg01_ext02_failure_after_apply_suspends_new_d_already_waiting() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    apply(&service, b"X").await;
    apply(&service, b"Y").await;
    let work = driver.open_external_work(T1, resource(103)).await.unwrap();
    let r = registered(&service, &A, T1, b"new-D").await;
    let gate = Gate::new();
    fixture
        .control
        .arm(Point::SettleBefore, Action::Pause(gate.clone()));
    let task_service = service.clone();
    let request_id = r.id();
    let task = tokio::spawn(async move {
        let mutation = submit(&task_service, b"new-D");
        task_service.consume(&A, T1, request_id, mutation).await
    });
    gate.wait().await;
    let before = view(&service, &B, T1).await;
    assert!(before.processing && before.applied);
    assert_eq!(before.waiting.len(), 3);
    assert!(!before.waiting[2].suspended);
    driver
        .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work })
        .await
        .unwrap();
    assert!(
        view(&service, &B, T1)
            .await
            .waiting
            .iter()
            .all(|e| e.suspended)
    );
    gate.release();
    assert!(matches!(
        outcome(&resolved(task.await.unwrap()).0),
        QueueOutcome::Queued { .. }
    ));
    assert!(
        view(&service, &B, T1)
            .await
            .waiting
            .iter()
            .all(|e| e.suspended)
    );
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl112_reg01_ext03_resume_and_failure_obey_actual_apply_order() {
    for failure_first in [true, false] {
        let fixture = Fixture::new().await;
        let (service, driver) = fixture.start().await;
        apply(&service, b"X").await;
        apply(&service, b"Y").await;
        let old = driver.open_external_work(T1, resource(103)).await.unwrap();
        driver
            .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work: old })
            .await
            .unwrap();
        let work = driver.open_external_work(T1, resource(103)).await.unwrap();
        let reg = service
            .register(&B, T1, &ValidatedMutation::Resume)
            .await
            .unwrap();
        let gate = Gate::new();
        fixture.control.arm(
            if failure_first {
                Point::BeginAfter
            } else {
                Point::SettleBefore
            },
            Action::Pause(gate.clone()),
        );
        let task_service = service.clone();
        let request_id = reg.id();
        let task = tokio::spawn(async move {
            task_service
                .consume(&B, T1, request_id, ValidatedMutation::Resume)
                .await
        });
        gate.wait().await;
        driver
            .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work })
            .await
            .unwrap();
        gate.release();
        assert_eq!(
            outcome(&resolved(task.await.unwrap()).0),
            QueueOutcome::Resumed { restored: 2 }
        );
        let snapshot = view(&service, &A, T1).await;
        assert_eq!(snapshot.failed, !failure_first);
        assert!(
            snapshot
                .waiting
                .iter()
                .all(|e| e.suspended != failure_first)
        );
        let (fact, _) = resolved(
            service
                .consume(&B, T1, reg.id(), ValidatedMutation::Resume)
                .await,
        );
        assert_eq!(outcome(&fact), QueueOutcome::Resumed { restored: 2 });
        assert_eq!(view(&service, &A, T1).await, snapshot);
        fixture.close(&service).await;
    }
}

#[tokio::test]
async fn cl110_reg02_ext04_ext05_external_keys_are_one_shot_with_new_same_resource_work_allowed() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    apply(&service, b"X").await;
    let old = driver.open_external_work(T1, resource(103)).await.unwrap();
    assert!(driver.open_external_work(T1, resource(103)).await.is_err());
    driver
        .observe(
            T1,
            QueueOwnerEvent::ExternalForegroundFailure { work: old.clone() },
        )
        .await
        .unwrap();
    let d = apply(&service, b"new-D").await;
    let before = view(&service, &A, T1).await;
    for event in [
        QueueOwnerEvent::ExternalForegroundFailure { work: old.clone() },
        QueueOwnerEvent::ExternalForegroundCompleted { work: old.clone() },
    ] {
        assert!(matches!(
            driver.observe(T1, event).await,
            Err(DriverError::Registry(RegistryError::StaleObservation))
        ));
        assert_eq!(view(&service, &A, T1).await, before);
    }
    let completed = driver.open_external_work(T1, resource(103)).await.unwrap();
    driver
        .observe(
            T1,
            QueueOwnerEvent::ExternalForegroundCompleted { work: completed },
        )
        .await
        .unwrap();
    assert_eq!(view(&service, &A, T1).await, before);
    let new = driver.open_external_work(T1, resource(103)).await.unwrap();
    assert_ne!(old.serial(), new.serial());
    driver
        .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work: new })
        .await
        .unwrap();
    let after = view(&service, &A, T1).await;
    assert!(
        after
            .waiting
            .iter()
            .find(|e| e.id == d.entity())
            .unwrap()
            .suspended
    );
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl110_reg03_ext07_external_failure_does_not_settle_local_active() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    let d = apply(&service, b"local-active-D").await;
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
    let prepared = driver.prepare(T1).await.unwrap().unwrap();
    let active = driver.begin_selection(&prepared).await.unwrap();
    apply(&service, b"X").await;
    apply(&service, b"Y").await;
    let work = driver.open_external_work(T1, resource(103)).await.unwrap();
    driver
        .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work })
        .await
        .unwrap();
    let snapshot = view(&service, &A, T1).await;
    assert_eq!(snapshot.active.as_ref().unwrap().id, d.entity());
    assert!(snapshot.waiting.iter().all(|e| e.suspended));
    assert_eq!(
        driver.active_key(T1).await.unwrap().unwrap().item(),
        active.item()
    );
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl113_reg01_evt10_direct_selection_is_unique_and_old_w_cannot_settle_new_d() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    apply(&service, b"W").await;
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
    let preparation = driver.prepare(T1).await.unwrap().unwrap();
    let w = driver.begin_selection(&preparation).await.unwrap();
    for bytes in [b"X", b"Y", b"Z"] {
        apply(&service, bytes).await;
    }
    driver
        .observe(
            T1,
            QueueOwnerEvent::Settle {
                active: w.clone(),
                disposition: QueueDisposition::Suspend,
            },
        )
        .await
        .unwrap();
    let d = registered(&service, &A, T1, b"D").await;
    let e = registered(&service, &B, T1, b"E").await;
    let a = service.clone();
    let b = service.clone();
    let (dr, er) = tokio::join!(
        async {
            let m = submit(&a, b"D");
            a.consume(&A, T1, d.id(), m).await
        },
        async {
            let m = submit(&b, b"E");
            b.consume(&B, T1, e.id(), m).await
        }
    );
    let outcomes = [outcome(&resolved(dr).0), outcome(&resolved(er).0)];
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, QueueOutcome::DirectSelected { .. }))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| **o == QueueOutcome::Rejected(QueueRejection::Full))
            .count(),
        1
    );
    let new = driver.active_key(T1).await.unwrap().unwrap();
    assert_ne!(new.item(), w.item());
    let before = view(&service, &A, T1).await;
    assert_eq!(before.waiting.len(), 3);
    assert!(before.waiting.iter().all(|x| x.suspended));
    for disposition in [QueueDisposition::Advance, QueueDisposition::Suspend] {
        assert!(matches!(
            driver
                .observe(
                    T1,
                    QueueOwnerEvent::Settle {
                        active: w.clone(),
                        disposition
                    }
                )
                .await,
            Err(DriverError::Registry(RegistryError::StaleObservation))
        ));
        assert_eq!(view(&service, &A, T1).await, before);
    }
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl110_reg01_evt12_environment_queued_failure_is_drained_before_new_apply() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    apply(&service, b"X").await;
    let work = driver.open_external_work(T1, resource(103)).await.unwrap();
    let r = registered(&service, &A, T1, b"new-D").await;
    let gate = Gate::new();
    fixture
        .control
        .arm(Point::BeginAfter, Action::Pause(gate.clone()));
    let s = service.clone();
    let task = tokio::spawn(async move {
        let m = submit(&s, b"new-D");
        s.consume(&A, T1, r.id(), m).await
    });
    gate.wait().await;
    fixture.environment.push(
        &native(1),
        QueueOwnerEvent::ExternalForegroundFailure { work },
    );
    gate.release();
    resolved(task.await.unwrap());
    let snapshot = view(&service, &A, T1).await;
    assert!(snapshot.waiting[0].suspended);
    assert!(!snapshot.waiting[1].suspended);
    let completed = driver.open_external_work(T1, resource(103)).await.unwrap();
    fixture.environment.push(
        &native(1),
        QueueOwnerEvent::ExternalForegroundCompleted {
            work: completed.clone(),
        },
    );
    service.poll_target(T1).await.unwrap();
    assert_eq!(view(&service, &A, T1).await, snapshot);
    assert!(matches!(
        driver
            .observe(
                T1,
                QueueOwnerEvent::ExternalForegroundFailure { work: completed }
            )
            .await,
        Err(DriverError::Registry(RegistryError::StaleObservation))
    ));
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl017_reg02_same_id_concurrent_and_applying_retries_use_one_real_operation() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let r = registered(&service, &A, T1, b"same-id").await;
    let request = r.id();
    let gate = Gate::new();
    fixture
        .control
        .arm(Point::BeginAfter, Action::Pause(gate.clone()));
    let first = service.clone();
    let task = tokio::spawn(async move {
        let body = submit(&first, b"same-id");
        first.consume(&A, T1, request, body).await
    });
    gate.wait().await;
    let second = service.clone();
    let duplicate = timeout(deadline(), async {
        let body = submit(&second, b"same-id");
        second.consume(&A, T1, request, body).await
    })
    .await
    .unwrap();
    let (fact, returned) = resolved(duplicate);
    assert!(matches!(fact,RequestFact::Applying(s) if s.registration.id()==request));
    assert!(returned.is_some());
    assert!(view(&service, &B, T1).await.waiting.is_empty());
    assert_eq!(fixture.sql_count("node_request_operations").await, 1);
    gate.release();
    resolved(task.await.unwrap());
    let snapshot = view(&service, &B, T1).await;
    assert_eq!(snapshot.waiting.len(), 1);
    assert_eq!(snapshot.waiting[0].id, request.entity());
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl104_reg03_lost_registration_response_can_leave_orphan_but_never_accepts_input() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    fixture
        .control
        .arm(Point::RegisterAfter, Action::ReturnUnconfirmed);
    let body = submit(&service, b"orphan-synthetic");
    assert!(matches!(
        service.register(&A, T1, &body).await,
        Err(RegistryError::CommitUnconfirmed)
    ));
    assert_eq!(fixture.sql_count("node_request_registrations").await, 1);
    assert_eq!(fixture.sql_count("node_request_operations").await, 0);
    assert!(view(&service, &B, T1).await.waiting.is_empty());
    let fresh = service.register(&A, T1, &body).await.unwrap();
    assert_eq!(fixture.sql_count("node_request_registrations").await, 2);
    resolved(service.consume(&A, T1, fresh.id(), body).await);
    assert_eq!(fixture.sql_count("node_request_operations").await, 1);
    assert_eq!(view(&service, &B, T1).await.waiting.len(), 1);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl075_reg03_real_engine_commit_failure_registered_inspection_needs_current_authority() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let r = registered(&service, &A, T1, b"commit-denied").await;
    let mut sql = connection(&fixture.area.db()).await;
    sqlx::query("CREATE TABLE independent_registry_commit_guard(actor TEXT REFERENCES clients(client_id) DEFERRABLE INITIALLY DEFERRED)").execute(&mut sql).await.unwrap();
    sqlx::query("CREATE TRIGGER independent_registry_begin_guard AFTER INSERT ON node_request_operations BEGIN INSERT INTO independent_registry_commit_guard VALUES('no-such-actor'); END").execute(&mut sql).await.unwrap();
    sql.close().await.unwrap();
    let response = service
        .consume(&A, T1, r.id(), submit(&service, b"commit-denied"))
        .await;
    assert!(matches!(response, ConsumeReply::Pending { .. }));
    assert_eq!(fixture.phase(r.id()).await, None);
    assert!(view(&service, &B, T1).await.waiting.is_empty());
    let mut sql = connection(&fixture.area.db()).await;
    sqlx::query("DROP TRIGGER independent_registry_begin_guard")
        .execute(&mut sql)
        .await
        .unwrap();
    sql.close().await.unwrap();
    fixture.revoke(A).await;
    assert_eq!(
        service.reconcile(T1, r.id()).await.unwrap(),
        RequestFact::Registered(r.clone())
    );
    match service
        .consume(&A, T1, r.id(), submit(&service, b"commit-denied"))
        .await
    {
        ConsumeReply::Refused {
            error,
            returned: ValidatedMutation::Submit(body),
        } => {
            assert_eq!(error, RegistryError::InvalidActor);
            assert_eq!(body.body().bytes, b"commit-denied");
        }
        _ => panic!("unadmitted registration reused obsolete authority or lost its returned body"),
    }
    assert_eq!(fixture.phase(r.id()).await, None);
    assert!(view(&service, &B, T1).await.waiting.is_empty());
    let report = service.shutdown(closing_deadline()).await;
    assert!(report.joined);
    fixture.storage.close().await;
}

#[tokio::test]
async fn cl075_reg04_complete_inspection_coordinates_must_match_before_recovering_once() {
    for corruption in [
        InspectionCorruption::Operation,
        InspectionCorruption::Runtime,
        InspectionCorruption::Generation,
        InspectionCorruption::BegunAt,
        InspectionCorruption::Intent,
    ] {
        let fixture = Fixture::new().await;
        let (service, _) = fixture.start().await;
        let r = registered(&service, &A, T1, b"inspect-binding").await;
        fixture
            .control
            .arm(Point::BeginAfter, Action::ReturnUnconfirmed);
        assert!(matches!(
            service
                .consume(&A, T1, r.id(), submit(&service, b"inspect-binding"))
                .await,
            ConsumeReply::Pending { .. }
        ));
        *fixture.control.inspection_corruption.lock().unwrap() = Some(corruption);
        assert!(service.reconcile(T1, r.id()).await.is_err());
        let snapshot = view(&service, &B, T1).await;
        assert!(snapshot.processing);
        assert!(snapshot.waiting.is_empty());
        assert_eq!(fixture.phase(r.id()).await.as_deref(), Some("applying"));
        service.reconcile(T1, r.id()).await.unwrap();
        assert_eq!(view(&service, &B, T1).await.waiting.len(), 1);
        assert_eq!(fixture.control.begin_calls.load(Ordering::SeqCst), 1);
        fixture.close(&service).await;
    }
}

#[tokio::test]
async fn cl087_reg01_pending_target_does_not_hold_read_or_other_target_behind_sql() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let r = registered(&service, &A, T1, b"pending-T1").await;
    let other = registered(&service, &B, T2, b"independent-T2").await;
    let gate = Gate::new();
    fixture
        .control
        .arm(Point::BeginAfter, Action::Pause(gate.clone()));
    let first = service.clone();
    let request = r.id();
    let task = tokio::spawn(async move {
        let body = submit(&first, b"pending-T1");
        first.consume(&A, T1, request, body).await
    });
    gate.wait().await;
    let blocked_view = timeout(deadline(), view(&service, &B, T1)).await.unwrap();
    assert!(blocked_view.processing && !blocked_view.applied);
    let second = timeout(
        deadline(),
        service.consume(&B, T2, other.id(), submit(&service, b"independent-T2")),
    )
    .await
    .unwrap();
    assert!(matches!(
        outcome(&resolved(second).0),
        QueueOutcome::Queued { .. }
    ));
    gate.release();
    resolved(task.await.unwrap());
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl087_reg03_real_target_binding_change_cannot_move_pending_content_to_another_owner() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let r = registered(&service, &A, T1, b"bound-target").await;
    let before = view(&service, &B, T1).await;
    let mut sql = connection(&fixture.area.db()).await;
    sqlx::query("UPDATE product_targets SET source_id=?,native_id='changed-native-target' WHERE target_id=?").bind(id(2).encode()).bind(T1.encode()).execute(&mut sql).await.unwrap();
    sql.close().await.unwrap();
    expect_rejected(
        service
            .consume(&A, T1, r.id(), submit(&service, b"bound-target"))
            .await,
        RegistryError::InvalidTarget,
    );
    assert_eq!(fixture.phase(r.id()).await, None);
    let mut sql = connection(&fixture.area.db()).await;
    sqlx::query(
        "UPDATE product_targets SET source_id=?,native_id='reg-q3-synthetic' WHERE target_id=?",
    )
    .bind(id(1).encode())
    .bind(T1.encode())
    .execute(&mut sql)
    .await
    .unwrap();
    sql.close().await.unwrap();
    assert_eq!(view(&service, &B, T1).await, before);
    assert!(view(&service, &B, T2).await.waiting.is_empty());
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl099_reg04_shutdown_stops_new_admission_and_preserves_unconfirmed_responsibility() {
    for point in [Point::BeginAfter, Point::SettleBefore, Point::SettleAfter] {
        let fixture = Fixture::new().await;
        let (service, driver) = fixture.start().await;
        let r = registered(&service, &A, T1, b"closing-pending").await;
        let request = r.id();
        let gate = Gate::new();
        fixture.control.arm(point, Action::Pause(gate.clone()));
        let owned = service.clone();
        let task = tokio::spawn(async move {
            let m = submit(&owned, b"closing-pending");
            owned.consume(&A, T1, request, m).await
        });
        gate.wait().await;
        let report = service
            .shutdown(
                acp_remote::application::request_registry::RegistryDeadline::new(
                    tokio::time::Instant::now() + tokio::time::Duration::from_millis(20),
                ),
            )
            .await;
        assert!(
            service
                .register(&B, T1, &submit(&service, b"must-not-admit"))
                .await
                .is_err()
        );
        assert!(driver.prepare(T1).await.is_err());
        if !report.joined {
            assert!(!report.unresolved.is_empty());
            assert!(matches!(
                fixture
                    .storage
                    .start_queue_service_with_audit_for_fixture(
                        fixture.authority.clone(),
                        Validator,
                        fixture.clock.clone(),
                        fixture.environment.clone(),
                        fixture.audit.clone()
                    )
                    .await,
                Err(RegistryError::RuntimeAlreadyStarted)
            ));
        }
        gate.release();
        let _ = timeout(deadline(), task).await;
        let final_report = service.shutdown(closing_deadline()).await;
        assert!(final_report.joined);
        let calls = fixture.control.settle_calls.load(Ordering::SeqCst);
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(fixture.control.settle_calls.load(Ordering::SeqCst), calls);
        fixture.storage.close().await;
    }
}

#[tokio::test]
async fn cl099_reg05_old_opaque_keys_are_rejected_after_joined_owner_reconstruction() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    apply(&service, b"old-W").await;
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
    let prepared = driver.prepare(T1).await.unwrap().unwrap();
    let active = driver.begin_selection(&prepared).await.unwrap();
    let work = driver.open_external_work(T1, resource(103)).await.unwrap();
    let report = service.shutdown(closing_deadline()).await;
    assert!(report.joined);
    let (new_service, new_driver) = fixture.start().await;
    let before = view(&new_service, &B, T1).await;
    assert!(before.waiting.is_empty() && before.active.is_none());
    assert!(new_driver.begin_selection(&prepared).await.is_err());
    assert!(matches!(
        new_driver
            .observe(
                T1,
                QueueOwnerEvent::Settle {
                    active,
                    disposition: QueueDisposition::Advance
                }
            )
            .await,
        Err(DriverError::Registry(RegistryError::StaleObservation))
    ));
    assert!(matches!(
        new_driver
            .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work })
            .await,
        Err(DriverError::Registry(RegistryError::StaleObservation))
    ));
    assert_eq!(view(&new_service, &B, T1).await, before);
    fixture.close(&new_service).await;
}

#[tokio::test]
async fn cl098_reg02_new_body_and_replaced_edit_are_not_persisted_in_main_or_wal() {
    const FIRST: &[u8] = b"REGISTRY_NEW_BODY_CANARY_8f3e89bd";
    const EDITED: &[u8] = b"REGISTRY_EDIT_BODY_CANARY_60f10fc2";
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let x = apply(&service, FIRST).await;
    let m = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 1,
        body: service.validate_body(EDITED).unwrap(),
    };
    let r = service.register(&B, T1, &m).await.unwrap();
    resolved(service.consume(&B, T1, r.id(), m).await);
    for entry in std::fs::read_dir(&fixture.area.0).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            for marker in [FIRST, EDITED] {
                assert!(
                    !bytes.windows(marker.len()).any(|window| window == marker),
                    "new synthetic body persisted in {}",
                    path.display()
                );
            }
        }
    }
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl095_reg01_actual_owner_accepts_large_body_without_old_content_budget() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let bytes = vec![b'Q'; 32 * 1024 * 1024 + 1];
    let r = apply(&service, &bytes).await;
    let snapshot = view(&service, &B, T1).await;
    assert_eq!(snapshot.waiting.len(), 1);
    assert_eq!(snapshot.waiting[0].id, r.entity());
    assert_eq!(snapshot.waiting[0].bytes, bytes);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl092_reg02_caller_abort_at_settle_boundaries_does_not_revoke_node_responsibility() {
    for point in [Point::SettleBefore, Point::SettleAfter] {
        let fixture = Fixture::new().await;
        let (service, _) = fixture.start().await;
        let r = registered(&service, &A, T1, b"caller-settle-drop").await;
        let request = r.id();
        let gate = Gate::new();
        fixture.control.arm(point, Action::Pause(gate.clone()));
        let s = service.clone();
        let task = tokio::spawn(async move {
            let m = submit(&s, b"caller-settle-drop");
            s.consume(&A, T1, request, m).await
        });
        gate.wait().await;
        let snapshot = view(&service, &B, T1).await;
        assert!(snapshot.processing && snapshot.applied);
        assert_eq!(snapshot.waiting.len(), 1);
        task.abort();
        let _ = task.await;
        gate.release();
        timeout(deadline(), async {
            loop {
                if !view(&service, &B, T1).await.processing {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(fixture.phase(request).await.as_deref(), Some("settled"));
        assert_eq!(view(&service, &B, T1).await.waiting.len(), 1);
        fixture.close(&service).await;
    }
}

fn failed_wrap<R: acp_remote::application::request_registry::RequestRegistryRepository>(
    inner: R,
) -> Result<Repository<R>, RegistryError> {
    drop(inner);
    Err(RegistryError::StorageUnavailable)
}
#[tokio::test]
async fn cl087_reg02_wrap_failure_releases_unused_claim_and_known_startup_failure_keeps_handle() {
    let fixture = Fixture::new().await;
    assert!(matches!(
        fixture
            .storage
            .start_queue_service_with_repository_and_audit_for_fixture(
                fixture.authority.clone(),
                Validator,
                fixture.clock.clone(),
                fixture.environment.clone(),
                fixture.audit.clone(),
                failed_wrap
            )
            .await,
        Err(RegistryError::StorageUnavailable)
    ));
    fixture
        .control
        .arm(Point::StartupBefore, Action::ReturnUnavailable);
    let (service, _) = fixture.start().await;
    assert_eq!(service.startup_status(), StartupStatus::Starting);
    assert!(matches!(
        fixture
            .storage
            .start_queue_service_with_audit_for_fixture(
                fixture.authority.clone(),
                Validator,
                fixture.clock.clone(),
                fixture.environment.clone(),
                fixture.audit.clone()
            )
            .await,
        Err(RegistryError::RuntimeAlreadyStarted)
    ));
    assert_eq!(
        service.reconcile_startup().await.unwrap(),
        StartupStatus::Running
    );
    apply(&service, b"after-known-startup-failure").await;
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl099_reg02_edit_invalidates_preparation_and_activity_prevents_further_edit() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    let x = apply(&service, b"X1").await;
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
    let old = driver.prepare(T1).await.unwrap().unwrap();
    let edit = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 1,
        body: service.validate_body(b"X2").unwrap(),
    };
    let r = service.register(&B, T1, &edit).await.unwrap();
    resolved(service.consume(&B, T1, r.id(), edit).await);
    assert!(matches!(
        driver.begin_selection(&old).await,
        Err(DriverError::Queue(QueueError::StaleToken))
    ));
    let next = driver.prepare(T1).await.unwrap().unwrap();
    let active = driver.begin_selection(&next).await.unwrap();
    assert_eq!(active.revision(), 2);
    let rejected = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 2,
        body: service.validate_body(b"X3-must-not-apply").unwrap(),
    };
    let r = service.register(&B, T1, &rejected).await.unwrap();
    assert_eq!(
        outcome(&resolved(service.consume(&B, T1, r.id(), rejected).await).0),
        QueueOutcome::Rejected(QueueRejection::DispatchStarted)
    );
    assert_eq!(view(&service, &B, T1).await.active.unwrap().bytes, b"X2");
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl099_reg03_evt12_new_submit_reads_readiness_after_real_sql_return() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    apply(&service, b"X").await;
    apply(&service, b"Y").await;
    let w = driver.open_external_work(T1, resource(103)).await.unwrap();
    driver
        .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work: w })
        .await
        .unwrap();
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
    let reg = registered(&service, &A, T1, b"D-latest-readiness").await;
    let request = reg.id();
    let gate = Gate::new();
    fixture
        .control
        .arm(Point::BeginAfter, Action::Pause(gate.clone()));
    let s = service.clone();
    let task = tokio::spawn(async move {
        let m = submit(&s, b"D-latest-readiness");
        s.consume(&A, T1, request, m).await
    });
    gate.wait().await;
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Blocked);
    fixture.environment.set_resource(&native(1), resource(104));
    fixture
        .environment
        .push(&native(1), QueueOwnerEvent::ActivityChanged);
    gate.release();
    let fact = resolved(task.await.unwrap()).0;
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Queued {
            item: request.entity(),
            revision: 1
        }
    );
    let snapshot = view(&service, &B, T1).await;
    assert!(snapshot.active.is_none());
    assert_eq!(snapshot.waiting.len(), 3);
    assert!(!snapshot.waiting[2].suspended);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl112_reg01_evt07_duplicate_noop_resume_cannot_clear_a_later_failure() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    apply(&service, b"X").await;
    apply(&service, b"Y").await;
    let w = driver.open_external_work(T1, resource(103)).await.unwrap();
    let reg = service
        .register(&B, T1, &ValidatedMutation::Resume)
        .await
        .unwrap();
    assert_eq!(
        outcome(
            &resolved(
                service
                    .consume(&B, T1, reg.id(), ValidatedMutation::Resume)
                    .await
            )
            .0
        ),
        QueueOutcome::Resumed { restored: 0 }
    );
    driver
        .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work: w })
        .await
        .unwrap();
    let before = view(&service, &A, T1).await;
    assert!(before.failed && before.waiting.iter().all(|e| e.suspended));
    assert_eq!(
        outcome(
            &resolved(
                service
                    .consume(&B, T1, reg.id(), ValidatedMutation::Resume)
                    .await
            )
            .0
        ),
        QueueOutcome::Resumed { restored: 0 }
    );
    assert_eq!(view(&service, &A, T1).await, before);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl112_reg02_resume_never_preempts_direct_active_and_places_waiting_new_before_old() {
    for direct in [false, true] {
        let fixture = Fixture::new().await;
        let (service, driver) = fixture.start().await;
        let x = apply(&service, b"X").await;
        let y = apply(&service, b"Y").await;
        let w = driver.open_external_work(T1, resource(103)).await.unwrap();
        driver
            .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work: w })
            .await
            .unwrap();
        if direct {
            fixture
                .environment
                .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
        }
        let d = registered(&service, &A, T1, b"D").await;
        let fact = resolved(
            service
                .consume(&A, T1, d.id(), submit(&service, b"D"))
                .await,
        )
        .0;
        assert_eq!(
            outcome(&fact),
            if direct {
                QueueOutcome::DirectSelected {
                    item: d.id().entity(),
                    revision: 1,
                }
            } else {
                QueueOutcome::Queued {
                    item: d.id().entity(),
                    revision: 1,
                }
            }
        );
        let r = service
            .register(&B, T1, &ValidatedMutation::Resume)
            .await
            .unwrap();
        assert_eq!(
            outcome(
                &resolved(
                    service
                        .consume(&B, T1, r.id(), ValidatedMutation::Resume)
                        .await
                )
                .0
            ),
            QueueOutcome::Resumed { restored: 2 }
        );
        let snapshot = view(&service, &A, T1).await;
        if direct {
            assert_eq!(snapshot.active.unwrap().id, d.id().entity());
            assert_eq!(
                waiting_ids(&view(&service, &A, T1).await),
                vec![x.entity(), y.entity()]
            );
            assert!(driver.prepare(T1).await.is_err());
        } else {
            assert_eq!(
                waiting_ids(&snapshot),
                vec![d.id().entity(), x.entity(), y.entity()]
            );
            fixture
                .environment
                .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
            let prepared = driver.prepare(T1).await.unwrap().unwrap();
            assert_eq!(
                driver.begin_selection(&prepared).await.unwrap().item(),
                d.id().entity()
            );
        }
        fixture.close(&service).await;
    }
}

#[tokio::test]
async fn cl113_reg02_full_suspended_queue_blocked_submission_cannot_hide_a_fourth_item() {
    let fixture = Fixture::new().await;
    let (service, driver) = fixture.start().await;
    for bytes in [b"X", b"Y", b"Z"] {
        apply(&service, bytes).await;
    }
    let w = driver.open_external_work(T1, resource(103)).await.unwrap();
    driver
        .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work: w })
        .await
        .unwrap();
    let before = view(&service, &A, T1).await;
    let d = registered(&service, &A, T1, b"D").await;
    assert_eq!(
        outcome(
            &resolved(
                service
                    .consume(&A, T1, d.id(), submit(&service, b"D"))
                    .await
            )
            .0
        ),
        QueueOutcome::Rejected(QueueRejection::Full)
    );
    assert_eq!(view(&service, &A, T1).await, before);
    fixture
        .environment
        .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
    assert_eq!(
        outcome(
            &resolved(
                service
                    .consume(&A, T1, d.id(), submit(&service, b"D"))
                    .await
            )
            .0
        ),
        QueueOutcome::Rejected(QueueRejection::Full)
    );
    assert_eq!(view(&service, &A, T1).await, before);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl111_reg02_actual_owner_new_d_result_keeps_or_suspends_remaining_items_without_requeue() {
    for disposition in [QueueDisposition::Advance, QueueDisposition::Suspend] {
        let fixture = Fixture::new().await;
        let (service, driver) = fixture.start().await;
        let x = apply(&service, b"X").await;
        let y = apply(&service, b"Y").await;
        let w = driver.open_external_work(T1, resource(103)).await.unwrap();
        driver
            .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work: w })
            .await
            .unwrap();
        let d = apply(&service, b"D").await;
        fixture
            .environment
            .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
        let preparation = driver.prepare(T1).await.unwrap().unwrap();
        let active = driver.begin_selection(&preparation).await.unwrap();
        assert_eq!(active.item(), d.entity());
        let e = apply(&service, b"E").await;
        driver
            .observe(
                T1,
                QueueOwnerEvent::Settle {
                    active,
                    disposition,
                },
            )
            .await
            .unwrap();
        let snapshot = view(&service, &B, T1).await;
        assert!(snapshot.active.is_none());
        assert_eq!(
            waiting_ids(&snapshot),
            vec![x.entity(), y.entity(), e.entity()]
        );
        assert!(snapshot.waiting[0].suspended && snapshot.waiting[1].suspended);
        assert_eq!(
            snapshot.waiting[2].suspended,
            disposition == QueueDisposition::Suspend
        );
        fixture.close(&service).await;
    }
}

#[tokio::test]
async fn cl112_reg03_ext08_uncertain_begin_or_settle_never_reapplies_or_replays_failure_events() {
    for point in [Point::BeginAfter, Point::SettleBefore, Point::SettleAfter] {
        let fixture = Fixture::new().await;
        let (service, driver) = fixture.start().await;
        apply(&service, b"X").await;
        apply(&service, b"Y").await;
        let work = driver.open_external_work(T1, resource(103)).await.unwrap();
        let r = registered(&service, &A, T1, b"D").await;
        fixture.control.arm(
            point,
            if point == Point::SettleBefore {
                Action::ReturnUnavailable
            } else {
                Action::ReturnUnconfirmed
            },
        );
        assert!(matches!(
            service
                .consume(&A, T1, r.id(), submit(&service, b"D"))
                .await,
            ConsumeReply::Pending { .. }
        ));
        driver
            .observe(
                T1,
                QueueOwnerEvent::ExternalForegroundFailure { work: work.clone() },
            )
            .await
            .unwrap();
        service.reconcile(T1, r.id()).await.unwrap();
        let snapshot = view(&service, &B, T1).await;
        assert_eq!(snapshot.waiting.len(), 3);
        assert!(snapshot.waiting[0].suspended && snapshot.waiting[1].suspended);
        assert_eq!(snapshot.waiting[2].suspended, point != Point::BeginAfter);
        assert!(matches!(
            driver
                .observe(T1, QueueOwnerEvent::ExternalForegroundFailure { work })
                .await,
            Err(DriverError::Registry(RegistryError::StaleObservation))
        ));
        assert_eq!(view(&service, &B, T1).await, snapshot);
        fixture.close(&service).await;
    }
}

#[tokio::test]
async fn cl018_reg01_same_actor_new_current_generation_reads_original_fact_without_new_apply() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let request = apply(&service, b"same-identity-new-trust-generation").await;
    let before = view(&service, &B, T1).await;
    let mut c = connection(&fixture.area.db()).await;
    sqlx::query("UPDATE clients SET auth_generation=17 WHERE client_id=?")
        .bind(A.id.encode())
        .execute(&mut c)
        .await
        .unwrap();
    c.close().await.unwrap();
    let current = Actor {
        id: A.id,
        generation: 17,
    };
    expect_rejected(
        service
            .consume(
                &A,
                T1,
                request,
                submit(&service, b"same-identity-new-trust-generation"),
            )
            .await,
        RegistryError::InvalidActor,
    );
    let (fact, returned) = resolved(
        service
            .consume(
                &current,
                T1,
                request,
                submit(&service, b"same-identity-new-trust-generation"),
            )
            .await,
    );
    assert!(returned.is_some());
    assert_eq!(fact.registration().unwrap().register_auth_generation(), 1);
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Queued {
            item: request.entity(),
            revision: 1
        }
    );
    assert_eq!(view(&service, &B, T1).await, before);
    assert_eq!(fixture.sql_count("node_request_operations").await, 1);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl104_reg02_legacy_command_identity_never_becomes_a_new_registration() {
    let fixture = Fixture::new().await;
    let legacy = id(96);
    let mut c = connection(&fixture.area.db()).await;
    sqlx::query("INSERT INTO product_commands(command_id,actor_id,auth_generation,request_id,request_epoch,action,target_kind,target_id,payload_digest,issued_at,expires_at,status,claim_id) VALUES(?,?,1,?,?,'session.input','session',?,?,0,1,'completed',NULL)")
        .bind(legacy.encode()).bind(A.id.encode()).bind(id(97).encode()).bind(id(98).encode())
        .bind(T1.encode()).bind("0".repeat(64)).execute(&mut c).await.unwrap();
    c.close().await.unwrap();
    let (service, _) = fixture.start().await;
    expect_rejected(
        service
            .consume(
                &A,
                T1,
                RegistrationId::new(legacy),
                submit(&service, b"legacy-is-not-a-ticket"),
            )
            .await,
        RegistryError::UnknownRegistration,
    );
    assert_eq!(fixture.sql_count("product_commands").await, 1);
    assert_eq!(fixture.sql_count("node_request_registrations").await, 0);
    assert_eq!(fixture.sql_count("node_request_operations").await, 0);
    assert!(view(&service, &A, T1).await.waiting.is_empty());
    let fresh = apply(&service, b"legacy-is-not-a-ticket").await;
    assert_ne!(fresh.entity(), legacy);
    assert_eq!(fixture.sql_count("product_commands").await, 1);
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl018_reg01_all_valid_edit_intent_coordinates_are_bound_before_queue_application() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let x = apply(&service, b"X-v1").await;
    let mutation = ValidatedMutation::Edit {
        item: x.entity(),
        revision: 1,
        body: service.validate_body(b"X-v2").unwrap(),
    };
    let registration = service.register(&B, T1, &mutation).await.unwrap();
    let before = view(&service, &B, T1).await;
    let attempts = [
        (
            T2,
            ValidatedMutation::Edit {
                item: x.entity(),
                revision: 1,
                body: service.validate_body(b"X-v2").unwrap(),
            },
        ),
        (
            T1,
            ValidatedMutation::Edit {
                item: id(95),
                revision: 1,
                body: service.validate_body(b"X-v2").unwrap(),
            },
        ),
        (
            T1,
            ValidatedMutation::Edit {
                item: x.entity(),
                revision: 2,
                body: service.validate_body(b"X-v2").unwrap(),
            },
        ),
        (
            T1,
            ValidatedMutation::Edit {
                item: x.entity(),
                revision: 1,
                body: service.validate_body(b"different").unwrap(),
            },
        ),
        (
            T1,
            ValidatedMutation::Cancel {
                item: x.entity(),
                revision: 1,
            },
        ),
    ];
    for (target, changed) in attempts {
        expect_rejected(
            service
                .consume(&B, target, registration.id(), changed)
                .await,
            RegistryError::Conflict,
        );
        assert_eq!(view(&service, &B, T1).await, before);
        assert_eq!(fixture.sql_count("node_request_operations").await, 1);
    }
    assert!(matches!(
        service
            .query_request(&B, T1, registration.id())
            .await
            .unwrap(),
        RequestFact::Registered(_)
    ));
    let (fact, _) = resolved(service.consume(&B, T1, registration.id(), mutation).await);
    assert_eq!(
        outcome(&fact),
        QueueOutcome::Edited {
            item: x.entity(),
            revision: 2
        }
    );
    fixture.close(&service).await;
}

#[tokio::test]
async fn cl110_reg01_evt01_evt03_local_active_failure_uses_current_waiting_set_during_pending_sql()
{
    for point in [Point::BeginAfter, Point::SettleBefore, Point::SettleAfter] {
        let fixture = Fixture::new().await;
        let (service, driver) = fixture.start().await;
        apply(&service, b"local-W").await;
        fixture
            .environment
            .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
        let preparation = driver.prepare(T1).await.unwrap().unwrap();
        let w = driver.begin_selection(&preparation).await.unwrap();
        fixture
            .environment
            .set_readiness(&native(1), DispatchReadiness::Blocked);
        let x = apply(&service, b"old-X").await;
        let y = apply(&service, b"old-Y").await;
        let r = registered(&service, &A, T1, b"new-D").await;
        let request = r.id();
        let gate = Gate::new();
        fixture.control.arm(point, Action::Pause(gate.clone()));
        let owned = service.clone();
        let waiter = tokio::spawn(async move {
            let m = submit(&owned, b"new-D");
            owned.consume(&A, T1, request, m).await
        });
        gate.wait().await;
        let before = view(&service, &B, T1).await;
        assert!(before.processing);
        assert_eq!(before.applied, point != Point::BeginAfter);
        driver
            .observe(
                T1,
                QueueOwnerEvent::Settle {
                    active: w.clone(),
                    disposition: QueueDisposition::Suspend,
                },
            )
            .await
            .unwrap();
        let current = view(&service, &B, T1).await;
        assert!(current.active.is_none());
        assert!(current.waiting.iter().all(|e| e.suspended));
        gate.release();
        assert_eq!(
            outcome(&resolved(waiter.await.unwrap()).0),
            QueueOutcome::Queued {
                item: request.entity(),
                revision: 1
            }
        );
        let after = view(&service, &B, T1).await;
        assert_eq!(
            waiting_ids(&after),
            vec![x.entity(), y.entity(), request.entity()]
        );
        assert!(after.waiting[0].suspended && after.waiting[1].suspended);
        assert_eq!(after.waiting[2].suspended, point != Point::BeginAfter);
        assert!(matches!(
            driver
                .observe(
                    T1,
                    QueueOwnerEvent::Settle {
                        active: w,
                        disposition: QueueDisposition::Advance
                    }
                )
                .await,
            Err(DriverError::Registry(RegistryError::StaleObservation))
        ));
        assert_eq!(view(&service, &B, T1).await, after);
        fixture.close(&service).await;
    }
}

#[tokio::test]
async fn cl112_reg01_evt05_evt06_local_settle_failure_and_resume_apply_obey_short_lock_order() {
    for failure_first in [true, false] {
        let fixture = Fixture::new().await;
        let (service, driver) = fixture.start().await;
        apply(&service, b"local-W").await;
        fixture
            .environment
            .set_readiness(&native(1), DispatchReadiness::Ready(resource(103)));
        let preparation = driver.prepare(T1).await.unwrap().unwrap();
        let w = driver.begin_selection(&preparation).await.unwrap();
        fixture
            .environment
            .set_readiness(&native(1), DispatchReadiness::Blocked);
        let x = apply(&service, b"X").await;
        let y = apply(&service, b"Y").await;
        let external = driver.open_external_work(T1, resource(103)).await.unwrap();
        driver
            .observe(
                T1,
                QueueOwnerEvent::ExternalForegroundFailure { work: external },
            )
            .await
            .unwrap();
        let r = service
            .register(&B, T1, &ValidatedMutation::Resume)
            .await
            .unwrap();
        let request = r.id();
        let gate = Gate::new();
        fixture.control.arm(
            if failure_first {
                Point::BeginAfter
            } else {
                Point::SettleBefore
            },
            Action::Pause(gate.clone()),
        );
        let owned = service.clone();
        let waiter = tokio::spawn(async move {
            owned
                .consume(&B, T1, request, ValidatedMutation::Resume)
                .await
        });
        gate.wait().await;
        driver
            .observe(
                T1,
                QueueOwnerEvent::Settle {
                    active: w,
                    disposition: QueueDisposition::Suspend,
                },
            )
            .await
            .unwrap();
        gate.release();
        assert_eq!(
            outcome(&resolved(waiter.await.unwrap()).0),
            QueueOutcome::Resumed { restored: 2 }
        );
        let after = view(&service, &B, T1).await;
        assert!(after.active.is_none());
        assert_eq!(waiting_ids(&after), vec![x.entity(), y.entity()]);
        assert_eq!(after.failed, !failure_first);
        assert!(after.waiting.iter().all(|e| e.suspended != failure_first));
        let (fact, returned) = resolved(
            service
                .consume(&B, T1, request, ValidatedMutation::Resume)
                .await,
        );
        assert_eq!(outcome(&fact), QueueOutcome::Resumed { restored: 2 });
        assert!(returned.is_some());
        assert_eq!(view(&service, &B, T1).await, after);
        fixture.close(&service).await;
    }
}

#[tokio::test]
async fn reg_shutdown_04_stop_after_actual_begin_commit_must_not_recover_unapplied_d() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let r = registered(&service, &A, T1, b"must-not-apply-after-stop").await;
    let request = r.id();
    let gate = Gate::new();
    fixture
        .control
        .arm(Point::BeginAfter, Action::Pause(gate.clone()));
    let owned = service.clone();
    let mut waiter = tokio::spawn(async move {
        let m = submit(&owned, b"must-not-apply-after-stop");
        owned.consume(&A, T1, request, m).await
    });
    gate.wait().await;
    assert_eq!(fixture.phase(request).await.as_deref(), Some("applying"));
    let before = view(&service, &B, T1).await;
    assert!(
        before.processing
            && !before.applied
            && before.waiting.is_empty()
            && before.active.is_none()
    );
    let first = service
        .shutdown(
            acp_remote::application::request_registry::RegistryDeadline::new(
                tokio::time::Instant::now(),
            ),
        )
        .await;
    if !first.joined {
        assert!(matches!(
            fixture
                .storage
                .start_queue_service_with_audit_for_fixture(
                    fixture.authority.clone(),
                    Validator,
                    fixture.clock.clone(),
                    fixture.environment.clone(),
                    fixture.audit.clone()
                )
                .await,
            Err(RegistryError::RuntimeAlreadyStarted)
        ));
    }
    gate.release();
    if timeout(deadline(), &mut waiter).await.is_err() {
        waiter.abort();
        let _ = waiter.await;
        panic!("stopped consume response waiter did not converge");
    }
    assert!(service.shutdown(closing_deadline()).await.joined);
    assert_eq!(fixture.phase(request).await.as_deref(), Some("applying"));
    assert_eq!(fixture.control.settle_calls.load(Ordering::SeqCst), 0);
    let (next, _) = fixture.start().await;
    assert!(matches!(
        next.query_request(&B, T1, request).await.unwrap(),
        RequestFact::Interrupted { .. }
    ));
    let empty = view(&next, &B, T1).await;
    assert!(empty.waiting.is_empty() && empty.active.is_none());
    assert_eq!(fixture.control.settle_calls.load(Ordering::SeqCst), 0);
    fixture.close(&next).await;
}

#[tokio::test]
async fn reg_shutdown_05_already_applied_edit_only_finishes_same_e_without_new_d() {
    for point in [Point::SettleBefore, Point::SettleAfter] {
        let fixture = Fixture::new().await;
        let (service, _) = fixture.start().await;
        let x = apply(&service, b"edit-before-stop-v1").await;
        let mutation = ValidatedMutation::Edit {
            item: x.entity(),
            revision: 1,
            body: service.validate_body(b"edit-before-stop-v2").unwrap(),
        };
        let r = service.register(&B, T1, &mutation).await.unwrap();
        fixture.control.arm(
            point,
            if point == Point::SettleBefore {
                Action::ReturnUnavailable
            } else {
                Action::ReturnUnconfirmed
            },
        );
        assert!(matches!(
            service.consume(&B, T1, r.id(), mutation).await,
            ConsumeReply::Pending { .. }
        ));
        let pending = view(&service, &B, T1).await;
        assert!(pending.processing && pending.applied);
        assert_eq!(pending.waiting[0].revision, 2);
        assert!(service.shutdown(closing_deadline()).await.joined);
        let relevant: Vec<_> = fixture
            .control
            .settle_values
            .lock()
            .unwrap()
            .iter()
            .filter(|(request, _)| *request == r.id())
            .map(|(_, outcome)| *outcome)
            .collect();
        assert_eq!(
            relevant,
            vec![
                QueueOutcome::Edited {
                    item: x.entity(),
                    revision: 2
                };
                if point == Point::SettleBefore { 2 } else { 1 }
            ]
        );
        let (next, _) = fixture.start().await;
        let fact = next.query_request(&B, T1, r.id()).await.unwrap();
        assert_eq!(
            outcome(&fact),
            QueueOutcome::Edited {
                item: x.entity(),
                revision: 2
            }
        );
        let empty = view(&next, &B, T1).await;
        assert!(empty.waiting.is_empty() && empty.active.is_none());
        fixture.close(&next).await;
    }
}
