use super::*;
use crate::domain::request_registry::RegistryError;
use crate::{
    domain::EntityId,
    session_storage::request_registry::{
        SqliteRequestRegistry,
        connection_close_test_hooks::{CloseBarrier, ClosePoint},
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};
fn deadline() -> RegistryDeadline {
    RegistryDeadline::new(tokio::time::Instant::now() + std::time::Duration::from_secs(5))
}
async fn registry() -> (
    SqliteRequestRegistry,
    sqlx::pool::PoolConnection<sqlx::Sqlite>,
) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(3)
        .min_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let keeper = pool.acquire().await.unwrap();
    (SqliteRequestRegistry::new(pool), keeper)
}
struct ScratchEvidence {
    reads: Arc<AtomicUsize>,
}
impl ReconcileEvidence for ScratchEvidence {
    fn request_id(&self) -> CatalogRequestId {
        CatalogRequestId::new(EntityId::from_bytes([7; 16]))
    }
    fn verify<'a>(
        &'a self,
        sql: &'a mut SqliteConnection,
    ) -> BoxFuture<'a, Result<(), CatalogError>> {
        Box::pin(async move {
            self.reads.fetch_add(1, Ordering::SeqCst);
            let n: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM main.sqlite_schema WHERE name='catalog_scratch'",
            )
            .fetch_one(sql)
            .await
            .map_err(|_| CatalogError::StorageUnavailable)?;
            if n != 0 {
                return Err(CatalogError::CorruptState);
            }
            Ok(())
        })
    }
}
#[tokio::test]
async fn node_configuration_sql_confirmed_commit_with_failed_report_keeps_same_receipt() {
    let (registry, _keeper) = registry().await;
    let fence = registry.catalog_fence();
    let d = deadline();
    let guard = fence
        .enter(CatalogOperationKind::Register, &registry.owner_scope(), &d)
        .await
        .unwrap();
    let op = guard.operation.clone();
    let mut c = registry
        .acquire_catalog_connection(op.clone(), CatalogSqlRole::RegisterWrite)
        .await
        .unwrap();
    c.begin_write().await.unwrap();
    sqlx::query("CREATE TABLE main.catalog_scratch(value INTEGER)")
        .execute(c.sql().unwrap())
        .await
        .unwrap();
    registry
        .connection_closer_for_test()
        .fail_next_result_after_ack()
        .unwrap();
    assert!(matches!(
        c.finish_catalog(Ok(()), &d).await,
        Err(CatalogError::SqlCloseUnconfirmed { committed: true })
    ));
    assert_eq!(
        op.failure(CatalogError::Cancelled).commit(),
        CommitKnowledge::Confirmed
    );
    drop(guard);
    let Err(failure) = fence
        .enter(CatalogOperationKind::Resolve, &registry.owner_scope(), &d)
        .await
    else {
        panic!("unresolved close must block");
    };
    assert_eq!(failure.commit(), CommitKnowledge::Confirmed);
    assert_eq!(
        failure.kind(),
        CatalogError::SqlCloseUnconfirmed { committed: true }
    );
    fence.recover(&registry, &d).await.unwrap();
    drop(
        fence
            .enter(CatalogOperationKind::Query, &registry.owner_scope(), &d)
            .await
            .unwrap(),
    );
    registry.close_connections().await;
}
#[tokio::test]
async fn node_configuration_sql_dropped_write_reconciles_once_and_recovery_close_is_retained() {
    let (registry, _keeper) = registry().await;
    let fence = registry.catalog_fence();
    let d = deadline();
    let reads = Arc::new(AtomicUsize::new(0));
    let guard = fence
        .enter(CatalogOperationKind::Apply, &registry.owner_scope(), &d)
        .await
        .unwrap();
    let op = guard.operation.clone();
    op.set_effect(Arc::new(ScratchEvidence {
        reads: reads.clone(),
    }));
    let mut c = registry
        .acquire_catalog_connection(op.clone(), CatalogSqlRole::ApplyWrite)
        .await
        .unwrap();
    c.begin_write().await.unwrap();
    sqlx::query("CREATE TABLE main.catalog_scratch(value INTEGER)")
        .execute(c.sql().unwrap())
        .await
        .unwrap();
    drop(c);
    drop(guard);
    assert_eq!(
        op.failure(CatalogError::Cancelled).commit(),
        CommitKnowledge::WritePossible
    );
    assert!(
        fence
            .enter(
                CatalogOperationKind::ResolveRead,
                &registry.owner_scope(),
                &d
            )
            .await
            .is_err()
    );
    // Wait for the actual original receipt, without changing its stored observation or opening SQL.
    let receipt = op.state().slots[0]
        .as_ref()
        .unwrap()
        .receipt
        .clone()
        .unwrap();
    assert_eq!(
        receipt.confirm_actual_cleanup(d.deadline()).await,
        OriginalCleanupObservation::ConfirmedJoined
    );
    registry
        .connection_closer_for_test()
        .fail_next_result_after_ack()
        .unwrap();
    assert!(fence.recover(&registry, &d).await.is_err());
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert!(
        fence
            .enter(CatalogOperationKind::Query, &registry.owner_scope(), &d)
            .await
            .is_err()
    );
    fence.recover(&registry, &d).await.unwrap();
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    drop(
        fence
            .enter(CatalogOperationKind::Resolve, &registry.owner_scope(), &d)
            .await
            .unwrap(),
    );
    registry.close_connections().await;
}
#[tokio::test]
async fn node_configuration_sql_caller_abort_preserves_commit_and_pending_original_close() {
    let (registry, _keeper) = registry().await;
    let fence = registry.catalog_fence();
    let barrier = CloseBarrier::new();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeClose, barrier.clone())
        .unwrap();
    let worker_registry = registry.clone();
    let worker_fence = fence.clone();
    let worker = tokio::spawn(async move {
        let d = deadline();
        let guard = worker_fence
            .enter(
                CatalogOperationKind::Apply,
                &worker_registry.owner_scope(),
                &d,
            )
            .await
            .unwrap();
        let mut c = worker_registry
            .acquire_catalog_connection(guard.operation.clone(), CatalogSqlRole::ApplyWrite)
            .await
            .unwrap();
        c.begin_write().await.unwrap();
        sqlx::query("CREATE TABLE main.catalog_scratch(value INTEGER)")
            .execute(c.sql().unwrap())
            .await
            .unwrap();
        let result = c.finish_catalog(Ok(()), &d).await;
        drop(guard);
        result
    });
    barrier.arrived().await;
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());
    let d = deadline();
    let Err(error) = fence
        .enter(CatalogOperationKind::Resolve, &registry.owner_scope(), &d)
        .await
    else {
        panic!("must remain blocked");
    };
    assert_eq!(error.commit(), CommitKnowledge::Confirmed);
    assert_eq!(error.cleanup(), CleanupKnowledge::UnconfirmedOriginal);
    let cancelled = deadline();
    cancelled.cancel();
    assert!(fence.recover(&registry, &cancelled).await.is_err());
    assert!(
        fence
            .enter(CatalogOperationKind::Resolve, &registry.owner_scope(), &d)
            .await
            .is_err()
    );
    barrier.release();
    fence.recover(&registry, &d).await.unwrap();
    drop(
        fence
            .enter(CatalogOperationKind::Resolve, &registry.owner_scope(), &d)
            .await
            .unwrap(),
    );
    registry.close_connections().await;
}
#[tokio::test]
async fn node_configuration_sql_shared_gate_serializes_whole_operation() {
    use std::{
        future::{Future, poll_fn},
        task::Poll,
    };
    let (registry, _keeper) = registry().await;
    let fence = registry.catalog_fence();
    let clone = registry.clone().catalog_fence();
    assert!(Arc::ptr_eq(&fence, &clone));
    let d = deadline();
    let first = fence
        .enter(CatalogOperationKind::Apply, &registry.owner_scope(), &d)
        .await
        .unwrap();
    let scope = registry.owner_scope();
    let second = clone.enter(CatalogOperationKind::Apply, &scope, &d);
    tokio::pin!(second);
    poll_fn(|cx| {
        assert!(second.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(first);
    drop(second.await.unwrap());
    registry.close_connections().await;
}

#[tokio::test]
async fn node_configuration_sql_rejects_temp_shadow_on_original_execution_connection() {
    let (registry, _keeper) = registry().await;
    let fence = registry.catalog_fence();
    let d = deadline();
    let guard = fence
        .enter(CatalogOperationKind::Query, &registry.owner_scope(), &d)
        .await
        .unwrap();
    let mut c = registry
        .acquire_catalog_connection(guard.operation.clone(), CatalogSqlRole::QueryRead)
        .await
        .unwrap();
    sqlx::query("CREATE TEMP TABLE clients(dummy INTEGER)")
        .execute(c.sql().unwrap())
        .await
        .unwrap();
    assert_eq!(
        c.begin_read().await.unwrap_err(),
        CatalogError::CorruptState
    );
    drop(c);
    drop(guard);
    assert!(
        fence
            .enter(CatalogOperationKind::Resolve, &registry.owner_scope(), &d)
            .await
            .is_err()
    );
    fence.recover(&registry, &d).await.unwrap();
    drop(
        fence
            .enter(CatalogOperationKind::Resolve, &registry.owner_scope(), &d)
            .await
            .unwrap(),
    );
    registry.close_connections().await;
}

#[tokio::test]
async fn node_configuration_sql_foreign_registry_cannot_confirm_or_reconcile_original_operation() {
    let (registry, _keeper) = registry().await;
    let (foreign, _foreign_keeper) = self::registry().await;
    let fence = registry.catalog_fence();
    let d = deadline();
    let guard = fence
        .enter(CatalogOperationKind::Query, &registry.owner_scope(), &d)
        .await
        .unwrap();
    let op = guard.operation.clone();
    let mut c = registry
        .acquire_catalog_connection(op.clone(), CatalogSqlRole::QueryRead)
        .await
        .unwrap();
    c.begin_read().await.unwrap();
    registry
        .connection_closer_for_test()
        .fail_next_result_after_ack()
        .unwrap();
    assert!(c.finish_catalog(Ok(()), &d).await.is_err());
    drop(guard);
    assert_eq!(
        fence.recover(&foreign, &d).await.unwrap_err().kind(),
        CatalogError::InvalidActor
    );
    assert_eq!(
        op.failure(CatalogError::Cancelled).cleanup(),
        CleanupKnowledge::UnconfirmedOriginal
    );
    fence.recover(&registry, &d).await.unwrap();
    registry.close_connections().await;
    foreign.close_connections().await;
}
#[tokio::test]
async fn node_configuration_sql_sealed_owner_allows_original_cleanup_but_no_new_sql() {
    let (registry, _keeper) = registry().await;
    let fence = registry.catalog_fence();
    let d = deadline();
    let guard = fence
        .enter(CatalogOperationKind::Query, &registry.owner_scope(), &d)
        .await
        .unwrap();
    let op = guard.operation.clone();
    let mut c = registry
        .acquire_catalog_connection(op.clone(), CatalogSqlRole::QueryRead)
        .await
        .unwrap();
    c.begin_read().await.unwrap();
    registry
        .connection_closer_for_test()
        .fail_next_result_after_ack()
        .unwrap();
    assert!(c.finish_catalog(Ok(()), &d).await.is_err());
    drop(guard);
    registry.close_connections().await;
    assert_eq!(
        fence.recover(&registry, &d).await.unwrap_err().kind(),
        CatalogError::NodeStopping
    );
    assert_eq!(
        op.failure(CatalogError::NodeStopping).cleanup(),
        CleanupKnowledge::ConfirmedOriginal
    );
    assert!(matches!(
        registry.catalog_operation_lease(),
        Err(RegistryError::NodeStopping)
    ));
}

#[tokio::test]
async fn node_configuration_sql_waiting_gate_stops_on_original_owner_seal() {
    use std::{
        future::{Future, poll_fn},
        task::Poll,
    };
    for kind in [
        CatalogOperationKind::Register,
        CatalogOperationKind::Apply,
        CatalogOperationKind::Query,
        CatalogOperationKind::Resolve,
        CatalogOperationKind::ResolveRead,
    ] {
        let (registry, _keeper) = registry().await;
        let fence = registry.catalog_fence();
        let d = deadline();
        let first_lease = registry.catalog_operation_lease().unwrap();
        let first = fence
            .enter(CatalogOperationKind::Apply, &registry.owner_scope(), &d)
            .await
            .unwrap();
        let second = async {
            let _lease = registry.catalog_operation_lease().unwrap();
            fence.enter(kind, &registry.owner_scope(), &d).await
        };
        tokio::pin!(second);
        poll_fn(|cx| {
            assert!(second.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        assert_eq!(fence.waiting.load(Ordering::SeqCst), 1);
        assert!(fence.serial.clone().try_lock_owned().is_err());
        registry.seal_connections();
        let result = tokio::time::timeout(std::time::Duration::from_millis(250), second.as_mut())
            .await
            .expect("gate waiter must stop while original gate remains held");
        assert_eq!(result.err().unwrap().kind(), CatalogError::NodeStopping);
        assert_eq!(fence.waiting.load(Ordering::SeqCst), 0);
        assert!(fence.serial.clone().try_lock_owned().is_err());
        assert!(
            first
                .operation
                .state
                .lock()
                .unwrap()
                .slots
                .iter()
                .all(Option::is_none)
        );
        assert!(Arc::ptr_eq(
            fence.retained.lock().unwrap().as_ref().unwrap(),
            &first.operation
        ));
        drop(first_lease);
        tokio::time::timeout(
            std::time::Duration::from_millis(250),
            registry.close_connections(),
        )
        .await
        .expect("waiter must release its original lease");
        assert_eq!(
            registry.close_status(),
            super::super::registry_connection_owner::ConnectionCloseStatus::Complete
        );
        drop(first);
    }
}
