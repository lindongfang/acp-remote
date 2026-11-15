//! Identity-only recovery regression on a genuine SQLite repository and actual owner.
#[allow(
    dead_code,
    reason = "Only a subset of the shared independent fixtures is used here."
)]
mod support;
use acp_remote::application::{
    queue_owner::ConsumeReply,
    request_registry::{QueueOutcome, RegistryError, RequestFact},
};
use support::*;
use tokio::time::timeout;

#[tokio::test]
async fn reg_owner_01_old_a_inspection_cannot_take_b_pending_or_its_worker() {
    let fixture = Fixture::new().await;
    let (service, _) = fixture.start().await;
    let a = registered(&service, &A, T1, b"owner-A").await;
    let a_id = a.id();
    let gate = Gate::new();
    fixture
        .control
        .arm(Point::BeginAfter, Action::Pause(gate.clone()));
    let first = service.clone();
    let task = tokio::spawn(async move {
        let m = submit(&first, b"owner-A");
        first.consume(&A, T1, a_id, m).await
    });
    gate.wait().await;
    let inspect = Gate::new();
    fixture
        .control
        .arm(Point::InspectBefore, Action::Pause(inspect.clone()));
    let stale = service.clone();
    let old = tokio::spawn(async move { stale.reconcile(T1, a_id).await });
    inspect.wait().await;
    gate.release();
    resolved(task.await.unwrap());
    let b = registered(&service, &B, T1, b"owner-B").await;
    fixture
        .control
        .arm(Point::BeginAfter, Action::ReturnUnconfirmed);
    assert!(matches!(
        service
            .consume(&B, T1, b.id(), submit(&service, b"owner-B"))
            .await,
        ConsumeReply::Pending { .. }
    ));
    let fact = service.query_request(&B, T1, b.id()).await.unwrap();
    assert!(matches!(fact, RequestFact::Applying(_)));
    let before = view(&service, &B, T1).await;
    assert!(before.processing && !before.applied);
    inspect.release();
    match timeout(deadline(), old).await.unwrap().unwrap() {
        Ok(fact) => assert_eq!(fact.registration().unwrap().id(), a_id),
        Err(error) => assert!(matches!(
            error,
            RegistryError::NoOwnedOperation | RegistryError::StaleObservation
        )),
    }
    assert_eq!(service.query_request(&B, T1, b.id()).await.unwrap(), fact);
    assert_eq!(view(&service, &B, T1).await, before);
    let recovered = timeout(deadline(), service.reconcile(T1, b.id()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        outcome(&recovered),
        QueueOutcome::Queued {
            item: b.id().entity(),
            revision: 1
        }
    );
    assert_eq!(
        view(&service, &B, T1)
            .await
            .waiting
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        vec![a_id.entity(), b.id().entity()]
    );
    assert_eq!(fixture.sql_count("node_request_operations").await, 2);
    fixture.close(&service).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reg_owner_02_parallel_a_recovery_and_b_admission_do_not_cross_operation_tickets() {
    for _ in 0..16 {
        let fixture = Fixture::new().await;
        let (service, _) = fixture.start().await;
        let a = registered(&service, &A, T1, b"parallel-A").await;
        let a_id = a.id();
        fixture
            .control
            .arm(Point::BeginAfter, Action::ReturnUnconfirmed);
        assert!(matches!(
            service
                .consume(&A, T1, a_id, submit(&service, b"parallel-A"))
                .await,
            ConsumeReply::Pending { .. }
        ));
        let b = registered(&service, &B, T1, b"parallel-B").await;
        let b_id = b.id();
        fixture
            .control
            .arm(Point::BeginAfter, Action::ReturnUnconfirmed);
        let mut recoveries = Vec::new();
        for _ in 0..8 {
            let old = service.clone();
            recoveries.push(tokio::spawn(async move { old.reconcile(T1, a_id).await }));
        }
        let next = service.clone();
        let next_waiter = tokio::spawn(async move {
            let m = submit(&next, b"parallel-B");
            next.consume(&B, T1, b_id, m).await
        });
        for recovery in recoveries {
            match timeout(deadline(), recovery).await.unwrap().unwrap() {
                Ok(fact) => assert_eq!(fact.registration().unwrap().id(), a_id),
                Err(error) => assert!(matches!(
                    error,
                    RegistryError::NoOwnedOperation | RegistryError::StaleObservation
                )),
            }
        }
        assert!(matches!(
            timeout(deadline(), next_waiter).await.unwrap().unwrap(),
            ConsumeReply::Pending { .. }
        ));
        let b = timeout(deadline(), service.reconcile(T1, b_id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            outcome(&b),
            QueueOutcome::Queued {
                item: b_id.entity(),
                revision: 1
            }
        );
        let ids = view(&service, &B, T1)
            .await
            .waiting
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![a_id.entity(), b_id.entity()]);
        assert_eq!(fixture.sql_count("node_request_operations").await, 2);
        fixture.close(&service).await;
    }
    println!("REG-OWNER-02 actual iterations=16, old A recovery calls=128");
}
