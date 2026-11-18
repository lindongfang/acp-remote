// Private overlay: child module of capacity_automatic::tests::rank_tests.
// Synthetic Created SQL facts are explicit development setup; replay/close IO
// and the necessary begin transaction are the actual production paths.
use super::*;
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;

#[tokio::test]
async fn rt_020a_new_b_then_actual_a_replay_withdraws_original_a_without_switching() {
    new_or_recovered_b(false).await;
}

#[tokio::test]
async fn rt_020b_recovered_b_then_actual_a_replay_withdraws_original_a_without_switching() {
    new_or_recovered_b(true).await;
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep the actual SQL pause, new/recovered eligibility and original-ticket assertions in one interleaving"
)]
async fn new_or_recovered_b(recovered: bool) {
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
    let result = AssertUnwindSafe(async {
        let core = &runtime.core;
        let source = core.ensure_source_slot(plan(&directory)).unwrap();
        core.repository
            .reserve_source(&source.pre, &core.deadline().unwrap())
            .await
            .unwrap();
        source.recorded.store(true, Ordering::Release);
        core.initialize_source_checked(&source, || Ok(()))
            .await
            .unwrap();
        let native_a = NativeSessionKey::from_config(Some(id(1)), "synthetic-native-A").unwrap();
        let native_b = NativeSessionKey::from_config(Some(id(1)), "synthetic-native-B").unwrap();
        let a_record = seed_synthetic_root(&storage, &source.pre).await;
        let a = core.capacity_resource(&source, &native_a).unwrap();
        *a.record.lock().unwrap() = Some(a_record.clone());
        core.reserve_native_capacity(&source, &a).await.unwrap();
        source.capacity_activity.lock().unwrap().created(
            native_a.native_id(),
            ProtocolVersion::V2,
            Instant::now(),
        );
        let a_original = core
            .automatic_candidate(&source, &a, Instant::now())
            .unwrap()
            .unwrap();
        let b_record = other_root(&storage, &source.pre).await;
        let b = core.capacity_resource(&source, &native_b).unwrap();
        *b.record.lock().unwrap() = Some(b_record.clone());
        core.reserve_native_capacity(&source, &b).await.unwrap();
        if recovered {
            // An explicitly installed current busy precondition. The transition
            // back to eligible below must come from actual reader/ACP replay.
            source.capacity_activity.lock().unwrap().created(
                native_b.native_id(),
                ProtocolVersion::V2,
                Instant::now(),
            );
            assert!(
                core.automatic_candidate(&source, &b, Instant::now())
                    .unwrap()
                    .is_some()
            );
            source
                .capacity_activity
                .lock()
                .unwrap()
                .observe(
                    &crate::application::native_dispatch::CapacityObservation {
                        session: Arc::from(native_b.native_id()),
                        origin: crate::application::native_dispatch::ObservationOrigin::Live,
                        freshness: crate::application::native_dispatch::CapacityFreshness::Current,
                        update: crate::application::native_dispatch::CapacityUpdate::State(
                            crate::application::native_dispatch::CapacityState {
                                foreground: crate::domain::session_capacity::Coverage::Active,
                                background: None,
                            },
                        ),
                    },
                    Instant::now(),
                )
                .unwrap();
        }
        assert_eq!(
            core.automatic_candidate(&source, &b, Instant::now())
                .unwrap(),
            None
        );
        let mut writer = storage.pool_for_test().begin().await.unwrap();
        sqlx::query("UPDATE product_sources SET name=name WHERE source_id=?")
            .bind(id(1).encode())
            .execute(&mut *writer)
            .await
            .unwrap();
        let (released, ()) = tokio::join!(
            core.reclaim_candidates(AutomaticReleaseReason::CapacityPressure),
            async {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while a.automatic.lock().unwrap().is_none() {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                assert!(
                    a.automatic
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .record
                        .lock()
                        .unwrap()
                        .is_none(),
                    "the original begin is actually waiting for SQLite"
                );
                let connection = source.source.get().unwrap();
                let setup = core
                    .protocol
                    .bind_setup(
                        connection,
                        ProtocolVersion::V2,
                        &source.plan.configuration,
                        source.plan.setups[0].1.clone(),
                    )
                    .unwrap();
                let before_b = source.memory.lock().unwrap().observed_sequence;
                let replay_b_started = Instant::now();
                let b_replay = connection
                    .replay_for_test(
                        native_b.native_id(),
                        &setup,
                        super::super::super::super::Instant::now() + Duration::from_secs(5),
                    )
                    .unwrap();
                assert_eq!(b_replay.wait().await, WriteReport::Written);
                tokio::time::timeout(Duration::from_secs(5), async {
                    while source.memory.lock().unwrap().observed_sequence < before_b + 5 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                let b_since = tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        if let Some(since) = core
                            .automatic_candidate(&source, &b, Instant::now())
                            .unwrap()
                        {
                            break since;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("actual current idle B consumed, not just a frame count");
                assert!(
                    b_since >= replay_b_started && b_since > a_original,
                    "new/recovered B cannot inherit a stale eligibility age"
                );
                let before_a = source.memory.lock().unwrap().observed_sequence;
                let a_replay = connection
                    .replay_for_test(
                        native_a.native_id(),
                        &setup,
                        super::super::super::super::Instant::now() + Duration::from_secs(5),
                    )
                    .unwrap();
                assert_eq!(a_replay.wait().await, WriteReport::Written);
                tokio::time::timeout(Duration::from_secs(5), async {
                    while source.memory.lock().unwrap().observed_sequence < before_a + 5 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                // automatic_candidate deliberately invalidates a retained/pending
                // operation and is not a read-only observer. Inspect only facts
                // received by the real pump; the original boundary re-evaluates.
                let a_since = tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        if let Some((foreground, since)) = source
                            .capacity_activity
                            .lock()
                            .unwrap()
                            .rt_current_foreground_for_test(native_a.native_id())
                            && foreground == crate::domain::session_capacity::Coverage::Idle
                            && since > b_since
                        {
                            break since;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("actual current idle A consumed after B's qualification");
                assert!(
                    a_since > b_since,
                    "actual A replay moved A behind newly eligible B"
                );
                assert_eq!(
                    core.automatic_candidate(&source, &b, Instant::now())
                        .unwrap(),
                    Some(b_since)
                );
                writer.commit().await.unwrap();
            }
        );
        assert!(
            !released.unwrap(),
            "the original selected A must be withdrawn"
        );
        let operation = a.automatic.lock().unwrap().clone().unwrap();
        assert_eq!(operation.original, a_record);
        assert_eq!(
            operation.record.lock().unwrap().as_ref().unwrap().phase,
            AutomaticReleasePhase::Withdrawn,
            "must reach original Zero withdrawal"
        );
        assert!(
            b.automatic.lock().unwrap().is_none(),
            "same attempt must not switch to B"
        );
        assert_eq!(*b.record.lock().unwrap(), Some(b_record));
        assert_eq!(
            core.repository
                .inspect_capacity(&core.deadline().unwrap())
                .await
                .unwrap()
                .occupied,
            2
        );
    })
    .catch_unwind()
    .await;
    let shutdown = runtime
        .shutdown(super::super::super::super::Instant::now() + Duration::from_secs(8))
        .await;
    drop(service);
    drop(runtime);
    storage.close().await;
    let _ = scope.stop_and_observe().await;
    assert!(
        shutdown.joined && shutdown.metadata_confirmed,
        "{shutdown:?}"
    );
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
