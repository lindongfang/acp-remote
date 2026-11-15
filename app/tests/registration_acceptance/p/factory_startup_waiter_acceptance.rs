//! REG-FACTORY public layer: delegate actual SQLite, then withhold the factory response.
//! SQL-before-call is explicitly separate from the same-crate true pre-COMMIT test.
#[path = "../factory_support.rs"]
#[allow(
    dead_code,
    reason = "The same decorator also serves the same-crate transaction harness."
)]
mod factory_support;
#[allow(
    dead_code,
    reason = "Shared test fixtures are used by several independent harness targets."
)]
mod support;
use acp_remote::{
    application::request_registry::*,
    domain::{EntityId, native::RuntimeEpoch},
};
use factory_support::{Gate, Stage, Stats, Tracked};
use sqlx::Connection;
use std::sync::{Arc, atomic::Ordering};
use support::{Fixture, T1, Validator, connection, deadline, id};
use tokio::time::timeout;

async fn seed_old_applying(fixture: &Fixture) -> RegistrationId {
    let request = RegistrationId::new(id(99));
    let mut sql = connection(&fixture.area.db()).await;
    sqlx::query("INSERT INTO node_request_registrations(request_id,intent_version,actor_id,register_auth_generation,action,target_id,registered_at_ms) VALUES(?,1,?,1,'input.resume',?,-10)").bind(request.encode()).bind(id(10).encode()).bind(T1.encode()).execute(&mut sql).await.unwrap();
    sqlx::query("INSERT INTO node_request_operations(request_id,operation_id,runtime_epoch,admission_auth_generation,phase,begun_at_ms) VALUES(?,?,?,1,'applying',-10)").bind(request.encode()).bind(id(98).encode()).bind(id(97).encode()).execute(&mut sql).await.unwrap();
    sql.close().await.unwrap();
    request
}

#[tokio::test]
async fn reg_factory_01_03_04_public_waiter_drop_preserves_storage_owned_startup_and_close_reaps_it()
 {
    for committed in [false, true] {
        let fixture = Arc::new(Fixture::new().await);
        let request = seed_old_applying(&fixture).await;
        let gate = Gate::new();
        let stats = Arc::new(Stats::default());
        let task_fixture = Arc::clone(&fixture);
        let task_gate = Arc::clone(&gate);
        let task_stats = Arc::clone(&stats);
        let task = tokio::spawn(async move {
            task_fixture
                .storage
                .start_queue_service_with_repository_and_audit_for_fixture(
                    task_fixture.authority.clone(),
                    Validator,
                    task_fixture.clock.clone(),
                    task_fixture.environment.clone(),
                    task_fixture.audit.clone(),
                    move |inner| {
                        Ok(Tracked::new(
                            inner,
                            task_stats,
                            if committed {
                                Stage::After(task_gate)
                            } else {
                                Stage::Before(task_gate)
                            },
                        ))
                    },
                )
                .await
        });
        timeout(deadline(), gate.arrived()).await.unwrap();
        assert_eq!(
            fixture.phase(request).await.as_deref(),
            Some(if committed { "interrupted" } else { "applying" })
        );
        assert_eq!(stats.wrapped.load(Ordering::SeqCst), 1);
        assert_eq!(stats.startup_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            stats.inner_returned.load(Ordering::SeqCst),
            u64::from(committed)
        );
        task.abort();
        assert!(
            task.await
                .err()
                .expect("factory waiter was not cancelled")
                .is_cancelled()
        );
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
        assert_eq!(stats.startup_calls.load(Ordering::SeqCst), 1);
        timeout(deadline(), fixture.storage.close()).await.expect(
            "Storage.close must cancel and join the hidden startup without a service handle",
        );
        assert!(
            stats.witness.lock().unwrap().upgrade().is_none(),
            "startup/service wrapper remained strongly owned after close"
        );
        let phase = fixture.phase(request).await;
        gate.release();
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(fixture.phase(request).await, phase);
        assert_eq!(stats.startup_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            stats.inner_returned.load(Ordering::SeqCst),
            u64::from(committed)
        );
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
            Err(RegistryError::NodeStopping)
        ));
    }
}
