//! REG-SHUTDOWN: concurrent close and cancellation of a close waiter retain ownership.
#[path = "../factory_support.rs"]
#[allow(
    dead_code,
    reason = "Only the repository lifetime observer is needed in this target."
)]
mod factory_support;
mod shutdown_support;
#[allow(
    dead_code,
    reason = "This harness uses a subset of shared independent fixtures."
)]
mod support;
use acp_remote::{
    application::request_registry::*,
    domain::{EntityId, native::RuntimeEpoch},
};
use factory_support::{Stage, Stats, Tracked};
use shutdown_support::{ClosingAudit, ClosingRepository, Counts, Gate, StopPoint};
use std::{
    future::{Future, poll_fn},
    sync::{Arc, atomic::Ordering},
    task::Poll,
};
use support::{A, B, Fixture, T1, Validator, deadline};
use tokio::time::{Duration, Instant, timeout};

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "The complete shutdown ownership sequence stays visible for independent review."
)]
async fn reg_shutdown_01_02_03_real_ba_sa_au_work_is_never_lost_to_competing_or_cancelled_close_waiters()
 {
    for (point, cancel_first) in [
        (StopPoint::Begin, false),
        (StopPoint::Settle, false),
        (StopPoint::Audit, false),
        (StopPoint::Begin, true),
        (StopPoint::Settle, true),
        (StopPoint::Audit, true),
    ] {
        let fixture = Fixture::new().await;
        let gate = Gate::new();
        let counts = Arc::new(Counts::default());
        let lifetime = Arc::new(Stats::default());
        let g = Arc::clone(&gate);
        let c = Arc::clone(&counts);
        let life = Arc::clone(&lifetime);
        let audit = Arc::new(ClosingAudit {
            gate: Arc::clone(&gate),
            point,
            counts: Arc::clone(&counts),
        });
        let (service, driver) = fixture
            .storage
            .start_queue_service_with_repository_and_audit_for_fixture(
                fixture.authority.clone(),
                Validator,
                fixture.clock.clone(),
                fixture.environment.clone(),
                audit,
                move |inner| {
                    Ok(Tracked::new(
                        ClosingRepository {
                            inner,
                            gate: g,
                            point,
                            counts: c,
                        },
                        life,
                        Stage::Pass,
                    ))
                },
            )
            .await
            .unwrap();
        let registration = service
            .register(
                &A,
                T1,
                &ValidatedMutation::Submit(service.validate_body(b"shutdown-owned-body").unwrap()),
            )
            .await
            .unwrap();
        let request = registration.id();
        let for_consume = service.clone();
        let mut consumer = tokio::spawn(async move {
            let mutation = ValidatedMutation::Submit(
                for_consume.validate_body(b"shutdown-owned-body").unwrap(),
            );
            for_consume.consume(&A, T1, request, mutation).await
        });
        timeout(deadline(), gate.wait()).await.unwrap();
        assert!(gate.live());
        assert_eq!(
            fixture.phase(request).await.as_deref(),
            Some(if point == StopPoint::Begin {
                "applying"
            } else {
                "settled"
            })
        );
        // Poll the actual public Future once, retain it locally, then cancel or
        // overlap another shutdown. It cannot complete in another spawned task
        // between reporting its Pending and our cancellation.
        let mut first = Some(Box::pin(service.shutdown(RegistryDeadline::new(
            Instant::now() + Duration::from_millis(80),
        ))));
        let first_poll = poll_fn(|cx| Poll::Ready(first.as_mut().unwrap().as_mut().poll(cx))).await;
        let was_pending = first_poll.is_pending();
        if let Poll::Ready(report) = first_poll {
            if report.joined {
                assert!(!gate.live());
            }
            drop(first.take());
        }
        if cancel_first {
            drop(first.take());
        }
        let second = service
            .shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_millis(80),
            ))
            .await;
        if second.joined {
            assert!(
                !gate.live(),
                "second shutdown falsely joined while the genuine work Future remains alive"
            );
        } else {
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
                Err(RegistryError::RuntimeAlreadyStarted | RegistryError::NodeStopping)
            ));
        }
        assert!(
            service
                .register(
                    &B,
                    T1,
                    &ValidatedMutation::Submit(
                        service.validate_body(b"after-stop-must-refuse").unwrap()
                    )
                )
                .await
                .is_err()
        );
        assert!(driver.prepare(T1).await.is_err());
        if let Some(pending) = first.take() {
            let report = pending.await;
            if report.joined {
                assert!(!gate.live());
            }
        }
        drop(first);
        timeout(deadline(), fixture.storage.close())
            .await
            .expect("Storage.close lost supervisor ownership after shutdown waiter cancellation");
        assert!(
            !gate.live(),
            "close returned while an owned SQL/audit Future still has a lifetime guard"
        );
        let phase = fixture.phase(request).await;
        let before = (
            counts.begin.load(Ordering::SeqCst),
            counts.settle.load(Ordering::SeqCst),
            counts.audit.load(Ordering::SeqCst),
        );
        gate.release();
        if timeout(deadline(), &mut consumer).await.is_err() {
            consumer.abort();
            let _ = consumer.await;
            panic!("consume response waiter did not finish after storage close");
        }
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(fixture.phase(request).await, phase);
        assert_eq!(
            (
                counts.begin.load(Ordering::SeqCst),
                counts.settle.load(Ordering::SeqCst),
                counts.audit.load(Ordering::SeqCst)
            ),
            before
        );
        drop(driver);
        drop(service);
        assert!(
            lifetime.witness.lock().unwrap().upgrade().is_none(),
            "service/supervisor retained a repository Arc cycle"
        );
        println!(
            "REG-SHUTDOWN {point:?}: cancel_first={cancel_first}, first_poll_pending={was_pending}, joined2={}",
            second.joined
        );
    }
}
