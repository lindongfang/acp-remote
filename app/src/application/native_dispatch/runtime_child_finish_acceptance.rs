// Core/SQLite scope. Synthetic original Created facts are explicit setup; the
// original ACP close ticket/result and the necessary finish are production IO.
use super::*;
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;

#[tokio::test]
async fn rt_018_actual_closed_sql_finish_abort_retains_original_until_same_retry() {
    let (storage, scope, directory) = storage_fixture().await;
    let configuration = Arc::new(storage.native_configuration_provider(Arc::new(Configuration)));
    let (service, runtime) = storage
        .start_native_service(
            Authority,
            Validator,
            SystemNodeClock,
            Arc::new(AcpNativeAdapter::default()),
            Arc::new(WindowsSourceLauncher),
            configuration,
            Arc::new(Retain),
            options(),
        )
        .await
        .unwrap();
    let pool = storage.pool_for_test().clone();
    let observed = AssertUnwindSafe(async {
        let core = &runtime.core;
        let source = core.ensure_source_slot(plan(&directory)).unwrap();
        core.repository.reserve_source(&source.pre, &core.deadline().unwrap()).await.unwrap();
        source.recorded.store(true, Ordering::Release);
        core.initialize_source_checked(&source, || Ok(())).await.unwrap();
        let native = NativeSessionKey::from_config(Some(id(1)), "synthetic-native-A").unwrap();
        let created = seed_synthetic_root(&storage, &source.pre).await;
        let capacity = core.capacity_resource(&source, &native).unwrap();
        *capacity.record.lock().unwrap() = Some(created.clone());
        core.reserve_native_capacity(&source, &capacity).await.unwrap();
        source.capacity_activity.lock().unwrap().created(native.native_id(), ProtocolVersion::V2, Instant::now());
        std::fs::write(directory.join("runtime-auto-count-close"), b"enabled").unwrap();
        sqlx::query("CREATE TRIGGER rt_automatic_finish_fault BEFORE UPDATE OF phase ON node_session_automatic_releases WHEN NEW.phase='finished' BEGIN SELECT RAISE(ABORT,'owned RT018 necessary finish fault'); END")
            .execute(&pool).await.unwrap();
        let first = core.automatic_release(&source, &capacity, AutomaticReleaseReason::CapacityPressure).await;
        assert!(first.is_err(), "necessary finish failure is not successful release: {first:?}");
        assert!(capacity.pre.authority.closed.load(Ordering::Acquire), "actual native Closed was received");
        let operation = capacity.automatic.lock().unwrap().clone().unwrap();
        let original = operation.id;
        assert_eq!(operation.original, created);
        let before: (String, i64, i64) = sqlx::query_as(
            "SELECT o.phase,c.physically_closed,c.released FROM node_session_automatic_releases o JOIN node_session_capacity c ON c.reservation_id=o.anchor_reservation WHERE o.operation_id=?"
        ).bind(original.encode()).fetch_one(&pool).await.unwrap();
        assert_eq!(before, ("pending".into(), 1, 0));
        assert_eq!(core.repository.inspect_capacity(&core.deadline().unwrap()).await.unwrap().occupied, 1);
        assert_eq!(std::fs::read_to_string(directory.join("rt-auto-close-count")).unwrap(), "1");
        sqlx::query("DROP TRIGGER rt_automatic_finish_fault").execute(&pool).await.unwrap();
        let retried = core.automatic_release(&source, &capacity, AutomaticReleaseReason::CapacityPressure).await;
        assert_eq!(retried, Ok(true));
        assert_eq!(capacity.automatic.lock().unwrap().as_ref().unwrap().id, original);
        let after: (String, i64, i64) = sqlx::query_as(
            "SELECT o.phase,c.physically_closed,c.released FROM node_session_automatic_releases o JOIN node_session_capacity c ON c.reservation_id=o.anchor_reservation WHERE o.operation_id=?"
        ).bind(original.encode()).fetch_one(&pool).await.unwrap();
        assert_eq!(after, ("finished".into(), 1, 1));
        assert_eq!(core.repository.inspect_capacity(&core.deadline().unwrap()).await.unwrap().occupied, 0);
        assert_eq!(core.automatic_release(&source, &capacity, AutomaticReleaseReason::CapacityPressure).await, Ok(true));
        assert_eq!(std::fs::read_to_string(directory.join("rt-auto-close-count")).unwrap(), "1", "retry never sends a second close");
    }).catch_unwind().await;
    sqlx::query("DROP TRIGGER IF EXISTS rt_automatic_finish_fault")
        .execute(&pool)
        .await
        .unwrap();
    let shutdown = runtime
        .shutdown(super::super::super::Instant::now() + Duration::from_secs(8))
        .await;
    assert!(
        shutdown.joined && shutdown.metadata_confirmed,
        "{shutdown:?}"
    );
    drop(service);
    drop(runtime);
    storage.close().await;
    scope.stop_and_observe().await;
    if let Err(error) = observed {
        std::panic::resume_unwind(error);
    }
}
