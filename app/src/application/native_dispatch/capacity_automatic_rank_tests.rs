use super::*;

async fn other_root(
    storage: &crate::storage::Storage,
    source: &Arc<super::super::super::PreSourceProbe>,
) -> CapacityRecord {
    let repository = storage.request_registry().native_dispatch_repository();
    let deadline = crate::application::request_registry::RegistryDeadline::new(
        super::super::super::Instant::now() + Duration::from_secs(5),
    );
    let reservation = pending_create(storage, source, id(91)).await;
    repository
        .reserve_capacity(&reservation, &deadline)
        .await
        .unwrap();
    sqlx::query("UPDATE node_source_session_operations SET protocol_version=2,rpc_id_kind='string',rpc_id_bytes=?,frame_length=X'0000000000000001',accepted_length=X'0000000000000001',transport_state='written',transport_recorded_at_ms=1,response_kind='created',native_session_id='synthetic-native-B',response_sequence=X'0000000000000001',response_receipt=X'0000000000000001',response_evidence=zeroblob(32),result_bytes=X'0000000000000001',ack_state='confirmed',operation_state='terminal',outcome='succeeded',terminal_at_ms=2 WHERE operation_id=?")
        .bind(id(91).encode().into_bytes()).bind(id(91).encode())
        .execute(storage.pool_for_test()).await.unwrap();
    repository
        .bind_created_capacity(
            id(91),
            &NativeSessionKey::from_config(Some(id(1)), "synthetic-native-B").unwrap(),
            source,
            &deadline,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn automatic_pressure_sql_wait_replay_passes_unchanged_older_b_then_withdraws_a() {
    pressure_sql_replay(true).await;
}

#[tokio::test]
async fn automatic_pressure_sql_wait_replay_keeps_sole_safe_candidate() {
    pressure_sql_replay(false).await;
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep the real SQLite wait, ACP replay and original release assertions in one controlled interleaving"
)]
async fn pressure_sql_replay(has_competitor: bool) {
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
    let native = NativeSessionKey::from_config(Some(id(1)), "synthetic-native-A").unwrap();
    let created = seed_synthetic_root(&storage, &source.pre).await;
    let a = core.capacity_resource(&source, &native).unwrap();
    *a.record.lock().unwrap() = Some(created);
    core.reserve_native_capacity(&source, &a).await.unwrap();
    source.capacity_activity.lock().unwrap().created(
        native.native_id(),
        ProtocolVersion::V2,
        Instant::now(),
    );
    let selected_since = core
        .automatic_candidate(&source, &a, Instant::now())
        .unwrap()
        .unwrap();
    let b = if has_competitor {
        let created = other_root(&storage, &source.pre).await;
        let native = NativeSessionKey::from_config(Some(id(1)), "synthetic-native-B").unwrap();
        let b = core.capacity_resource(&source, &native).unwrap();
        *b.record.lock().unwrap() = Some(created);
        core.reserve_native_capacity(&source, &b).await.unwrap();
        source.capacity_activity.lock().unwrap().created(
            native.native_id(),
            ProtocolVersion::V2,
            Instant::now(),
        );
        let since = core
            .automatic_candidate(&source, &b, Instant::now())
            .unwrap()
            .unwrap();
        assert!(
            selected_since < since,
            "A is selected before B at the original ranking"
        );
        Some((b, since))
    } else {
        None
    };
    // Hold a real SQLite writer while begin_automatic_release is waiting. This
    // does not replace the production repository or manufacture a transport fact.
    let mut writer = storage.pool_for_test().begin().await.unwrap();
    sqlx::query("UPDATE product_sources SET name=name WHERE source_id=?")
        .bind(id(1).encode())
        .execute(&mut *writer)
        .await
        .unwrap();
    let worker = {
        let core = core.clone();
        tokio::spawn(async move {
            core.reclaim_candidates(AutomaticReleaseReason::CapacityPressure)
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if a.automatic.lock().unwrap().is_some() {
                break;
            }
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
            .is_none()
    );
    let before_replay = source.memory.lock().unwrap().observed_sequence;
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
    let replay = connection
        .replay_for_test(
            native.native_id(),
            &setup,
            super::super::super::Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(replay.wait().await, WriteReport::Written);
    tokio::time::timeout(Duration::from_secs(5), async {
        while source.memory.lock().unwrap().observed_sequence < before_replay + 5 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    if let Some((b, since)) = &b {
        assert_eq!(
            core.automatic_candidate(&source, b, Instant::now())
                .unwrap(),
            Some(*since)
        );
    }
    writer.commit().await.unwrap();
    let released = tokio::time::timeout(Duration::from_secs(10), worker)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let original = a.automatic.lock().unwrap().clone().unwrap();
    let phase = original.record.lock().unwrap().as_ref().unwrap().phase;
    let occupied = core
        .repository
        .inspect_capacity(&core.deadline().unwrap())
        .await
        .unwrap()
        .occupied;
    let b_unattempted = b
        .as_ref()
        .is_none_or(|(b, _)| b.automatic.lock().unwrap().is_none());
    let report = runtime
        .shutdown(super::super::super::Instant::now() + Duration::from_secs(5))
        .await;
    assert!(report.joined && report.metadata_confirmed, "{report:?}");
    drop(service);
    drop(runtime);
    storage.close().await;
    scope.stop_and_observe().await;
    assert!(
        b_unattempted,
        "same pressure request must never switch to B"
    );
    if has_competitor {
        assert!(
            !released,
            "A replay moved it behind unchanged B during SQL wait"
        );
        assert_eq!(phase, AutomaticReleasePhase::Withdrawn);
        assert_eq!(occupied, 2, "Zero withdrawal returns no capacity");
    } else {
        assert!(
            released,
            "SQL-wait replay cannot permanently exempt the only safe candidate"
        );
        assert_eq!(phase, AutomaticReleasePhase::Finished);
        assert_eq!(occupied, 0);
    }
}

#[path = "runtime_child_sql_rank_acceptance.rs"]
mod runtime_child_sql_rank_acceptance;
