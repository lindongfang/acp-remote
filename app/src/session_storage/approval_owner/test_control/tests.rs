//! Controller component checks; these data-only cuts are not native release or dispatch proof.
use super::*;
use crate::{error::StorageError, session_storage::schema};
use std::time::Duration;
fn tag() -> ApprovalOperationTag {
    ApprovalOperationTag {
        operation: EntityId::from_bytes([1; 16]),
        kind: ApprovalSqlOperation::Recheck,
        instance: None,
        approval: None,
        registration: None,
        response: None,
    }
}
#[tokio::test]
async fn approval_controller_none_source_and_both_notification_orders() {
    let controller = Arc::new(ApprovalTestController::new_registry_controller());
    let point = ApprovalCut::ReconcileBeforeBegin;
    let unmatched = controller
        .pause_next(
            point,
            ApprovalCutSelector::SourceInstance(EntityId::from_bytes([2; 16])),
        )
        .unwrap();
    controller
        .hit(point, tag(), ApprovalCutState::before_begin())
        .await
        .unwrap();
    assert!(controller.events_after(0).is_empty());
    assert!(matches!(
        unmatched
            .wait_until_arrived(Instant::now() + Duration::from_millis(5))
            .await,
        Err(ApprovalWaitError::TimedOut)
    ));
    drop(unmatched);
    let first = controller
        .pause_next(point, ApprovalCutSelector::Next)
        .unwrap();
    let first_id = first.id();
    first.release();
    controller
        .hit(point, tag(), ApprovalCutState::before_begin())
        .await
        .unwrap();
    assert_eq!(
        first
            .wait_until_arrived(Instant::now() + Duration::from_secs(1))
            .await
            .unwrap()
            .arm,
        first_id
    );
    let second = controller
        .pause_next(point, ApprovalCutSelector::Next)
        .unwrap();
    assert_ne!(second.id(), first_id);
    drop(first);
    let original = controller.clone();
    let pending = tokio::spawn(async move {
        original
            .hit(point, tag(), ApprovalCutState::before_begin())
            .await
    });
    let arrival = second
        .wait_until_arrived(Instant::now() + Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(arrival.arm, second.id());
    assert!(!arrival.source_job_registered);
    assert!(controller.fail_result_after_ack(&second).is_err());
    second.release();
    pending.await.unwrap().unwrap();
    let third = controller
        .pause_next(point, ApprovalCutSelector::Next)
        .unwrap();
    let original = controller.clone();
    let stopped = tokio::spawn(async move {
        original
            .hit(point, tag(), ApprovalCutState::before_begin())
            .await
    });
    third
        .wait_until_arrived(Instant::now() + Duration::from_secs(1))
        .await
        .unwrap();
    controller.stop();
    assert!(matches!(
        stopped.await.unwrap(),
        Err(ApprovalControlError::Stopped)
    ));
}
async fn pool() -> sqlx::SqlitePool {
    sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap()
}
#[tokio::test]
async fn approval_controller_twelve_real_ddl_before_after_visits_rollback() {
    assert!(ApprovalDdlOrdinal::new(0).is_err());
    assert!(ApprovalDdlOrdinal::new(13).is_err());
    for n in 1..=12 {
        for edge in [ApprovalDdlEdge::Before, ApprovalDdlEdge::After] {
            let pool = pool().await;
            let trace = ApprovalMigrationTrace::new();
            let fault = ApprovalMigrationFault::Ddl {
                ordinal: ApprovalDdlOrdinal::new(n).unwrap(),
                edge,
            };
            assert!(matches!(
                schema::initialize_approval_for_test(&pool, Some(fault), trace.clone()).await,
                Err(StorageError::MigrationInterrupted)
            ));
            let visits = trace.entries();
            assert_eq!(
                visits.len(),
                usize::from(n) * 2 - usize::from(edge == ApprovalDdlEdge::Before)
            );
            let final_visit = visits.last().unwrap();
            assert_eq!(final_visit.ordinal.get(), n);
            assert_eq!(final_visit.edge, edge);
            assert_eq!(
                final_visit.actual_ddl_succeeded,
                edge == ApprovalDdlEdge::After
            );
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM main.sqlite_master WHERE name NOT LIKE 'sqlite_%'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(count, 0);
            let version: i64 = sqlx::query_scalar("PRAGMA main.user_version")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(version, 0);
            pool.close().await;
        }
    }
}
#[tokio::test]
async fn approval_controller_migration_true_commit_787_and_after_commit_head8() {
    let database = pool().await;
    let trace = ApprovalMigrationTrace::new();
    let error = schema::initialize_approval_for_test(
        &database,
        Some(ApprovalMigrationFault::DeferredCommit),
        trace.clone(),
    )
    .await
    .unwrap_err();
    let StorageError::Database(error) = error else {
        panic!("expected actual SQLite COMMIT failure");
    };
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("787")
    );
    assert_eq!(trace.entries().len(), 24);
    database.close().await;
    let database = pool().await;
    let trace = ApprovalMigrationTrace::new();
    assert!(
        schema::initialize_approval_for_test(
            &database,
            Some(ApprovalMigrationFault::AfterCommitReportLost),
            trace.clone()
        )
        .await
        .is_err()
    );
    let version: i64 = sqlx::query_scalar("PRAGMA main.user_version")
        .fetch_one(&database)
        .await
        .unwrap();
    assert_eq!(version, crate::storage::SCHEMA_VERSION);
    assert_eq!(trace.entries().len(), 24);
    database.close().await;
}
