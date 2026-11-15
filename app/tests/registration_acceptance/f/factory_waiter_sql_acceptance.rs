//! REG-FACTORY true pre/post-COMMIT cases, same-crate closed hooks on actual SQLite.
#[path = "../factory_support.rs"]
#[allow(
    dead_code,
    reason = "The same decorator also serves the public startup harness."
)]
mod factory_support;
#[path = "factory_fixtures.rs"]
mod fixtures;
use crate::{
    application::{queue_owner::BlockedEnvironment, request_registry::*},
    domain::{
        ActorContext, EntityId,
        native::{NativeSessionKey, RuntimeEpoch},
    },
    session_storage::request_registry::test_hooks::{Action, Barrier, Operation, Point},
    storage::Storage,
};
use factory_support::{Stage, Stats, Tracked};
use fixtures::{Area, connect, fixture_base};
use sqlx::Connection;
use std::sync::{Arc, atomic::Ordering};
use tokio::time::{Duration, timeout};

const fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
struct Authority;
#[allow(
    clippy::unused_async_trait_impl,
    reason = "Trusted fixture follows the asynchronous port without artificial yields."
)]
impl RequestAuthority for Authority {
    type Actor = ActorContext;
    async fn authorize(
        &self,
        a: &ActorContext,
        _: EntityId,
        _: QueueAction,
    ) -> Result<ActorContext, RegistryError> {
        Ok(*a)
    }
    async fn authorize_read(
        &self,
        a: &ActorContext,
        _: EntityId,
    ) -> Result<ActorContext, RegistryError> {
        Ok(*a)
    }
    async fn resolve_target(&self, _: EntityId) -> Result<NativeSessionKey, RegistryError> {
        NativeSessionKey::from_config(Some(id(80)), "factory-actual-sql")
            .map_err(|_| RegistryError::InvalidTarget)
    }
}
struct Validator;
impl ValidatedBodyPort for Validator {
    type Raw = ();
    type Body = ();
    fn validate(&self, (): &()) -> Result<((), [u8; 32]), RegistryError> {
        Ok(((), [0; 32]))
    }
}
struct Clock;
impl NodeClock for Clock {
    fn unix_millis(&self) -> i64 {
        1000
    }
}
async fn phase(area: &Area, request: RegistrationId) -> String {
    let mut c = connect(&area.db()).await;
    let result = sqlx::query_scalar("SELECT phase FROM node_request_operations WHERE request_id=?")
        .bind(request.encode())
        .fetch_one(&mut c)
        .await
        .unwrap();
    c.close().await.unwrap();
    result
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep each true COMMIT boundary and its lifetime evidence together."
)]
async fn reg_factory_02_03_04_true_sql_before_and_after_commit_drop_waiter_close_joins_detached_work()
 {
    for after_commit in [false, true] {
        let area = Area::new();
        fixture_base(&area, 3).await;
        let storage = Arc::new(Storage::open(&area.db()).await.unwrap());
        let mut c = connect(&area.db()).await;
        sqlx::query("INSERT INTO product_sources VALUES(?,'factory-synthetic-source')")
            .bind(id(80).encode())
            .execute(&mut c)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_targets VALUES(?,?, 'factory-actual-sql',NULL)")
            .bind(id(81).encode())
            .bind(id(80).encode())
            .execute(&mut c)
            .await
            .unwrap();
        let request = RegistrationId::new(id(99));
        sqlx::query("INSERT INTO node_request_registrations(request_id,intent_version,actor_id,register_auth_generation,action,target_id,registered_at_ms) VALUES(?,1,?,12,'input.resume',?,-10)").bind(request.encode()).bind(id(1).encode()).bind(id(81).encode()).execute(&mut c).await.unwrap();
        sqlx::query("INSERT INTO node_request_operations(request_id,operation_id,runtime_epoch,admission_auth_generation,phase,begun_at_ms) VALUES(?,?,?,12,'applying',-10)").bind(request.encode()).bind(id(98).encode()).bind(id(97).encode()).execute(&mut c).await.unwrap();
        c.close().await.unwrap();
        let barrier = Barrier::new();
        let stats = Arc::new(Stats::default());
        let for_task = Arc::clone(&storage);
        let gate = Arc::clone(&barrier);
        let counts = Arc::clone(&stats);
        let task = tokio::spawn(async move {
            for_task
                .start_queue_service_with_repository_and_audit_for_fixture(
                    Authority,
                    Validator,
                    Clock,
                    Arc::new(BlockedEnvironment),
                    Arc::new(NoAudit),
                    move |inner| {
                        inner.faults_for_test().arm(
                            if after_commit {
                                Point::AfterCommit(Operation::Startup)
                            } else {
                                Point::BeforeCommit(Operation::Startup)
                            },
                            Action::Pause(gate),
                        )?;
                        Ok(Tracked::new(inner, counts, Stage::Pass))
                    },
                )
                .await
        });
        timeout(Duration::from_secs(20), barrier.arrived())
            .await
            .unwrap();
        assert_eq!(
            phase(&area, request).await,
            if after_commit {
                "interrupted"
            } else {
                "applying"
            }
        );
        task.abort();
        assert!(
            task.await
                .err()
                .expect("factory waiter was not cancelled")
                .is_cancelled()
        );
        assert!(matches!(
            storage
                .start_queue_service_with_audit_for_fixture(
                    Authority,
                    Validator,
                    Clock,
                    Arc::new(BlockedEnvironment),
                    Arc::new(NoAudit)
                )
                .await,
            Err(RegistryError::RuntimeAlreadyStarted)
        ));
        timeout(Duration::from_secs(20), storage.close())
            .await
            .expect("hidden factory startup must remain cancellable and joinable through Storage");
        assert!(stats.witness.lock().unwrap().upgrade().is_none());
        let stable = phase(&area, request).await;
        assert_eq!(
            stable,
            if after_commit {
                "interrupted"
            } else {
                "applying"
            }
        );
        let mut c = connect(&area.db()).await;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut c)
            .await
            .expect("detached startup connection retained a writer lock after close");
        sqlx::query("ROLLBACK").execute(&mut c).await.unwrap();
        c.close().await.unwrap();
        barrier.release();
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(phase(&area, request).await, stable);
        assert_eq!(stats.wrapped.load(Ordering::SeqCst), 1);
        assert_eq!(stats.startup_calls.load(Ordering::SeqCst), 1);
        assert_eq!(stats.inner_returned.load(Ordering::SeqCst), 0);
    }
}
