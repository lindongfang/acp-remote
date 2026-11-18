use super::*;
use crate::application::{queue_owner::QueueService, shared_views::budget::ReadReason};
use crate::session_storage::{
    native_dispatch::SqliteNativeDispatch, request_registry::SqliteRequestRegistry,
};

struct ChildReadRelease(PathBuf);
impl Drop for ChildReadRelease {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, b"release");
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Track both child views and their original root task ownership across read, seal and join"
)]
pub(super) async fn child_read_scenario<L>(
    storage: &crate::sqlite_fixture_scope::FixtureStorage,
    service: &QueueService<Authority, Validator, SqliteRequestRegistry, SystemNodeClock>,
    native: &NativeRuntimeHandle<
        AcpNativeAdapter,
        L,
        SqliteNativeConfigurationProvider<Configuration>,
        SqliteNativeDispatch,
    >,
    area: &NativeArea,
    scenario: &str,
) where
    L: SourceLaunchPort<Plan = WindowsLaunchPlan>,
    L::Owned: OwnedNativeSource<Streams = AcpStreams>,
{
    if scenario.starts_with("child-auto-read-rt-race-") {
        return capacity_registration_race_acceptance::run(
            storage, service, native, area, scenario,
        )
        .await;
    }
    if scenario.starts_with("child-auto-read-rt") {
        return runtime_child_join_acceptance::rt_case(storage, service, native, area, scenario)
            .await;
    }
    native
        .install_view_factory(Arc::new(
            crate::adapters::acp::native_views::AcpNativeViewFactory::replay_only_for_test(
                std::num::NonZeroUsize::new(64).unwrap(),
                Arc::new(BridgeReadOptions::new()),
            ),
        ))
        .unwrap();
    let proof = service
        .prepare_native_read(&id(10), id(4), ReadReason::FirstOpen)
        .await
        .unwrap();
    let plan = native
        .core
        .configuration
        .resolve_read(&proof)
        .await
        .unwrap();
    let source = native
        .core
        .capacity_source(proof.native(), Arc::new(plan.into_source()))
        .await
        .unwrap();
    native
        .core
        .repository
        .reserve_source(&source.pre, &native.core.deadline().unwrap())
        .await
        .unwrap();
    source.recorded.store(true, Ordering::Release);
    seed_child_read_tree(storage.pool_for_test(), &source.pre).await;
    assert!(
        source.source.get().is_none(),
        "root fixture has no native connection or body"
    );
    for (target, child) in [(id(8), "synthetic-child-A"), (id(5), "synthetic-child-B")] {
        sqlx::query("INSERT INTO product_targets(target_id,source_id,native_id,workspace_id) VALUES(?,?,?,?) ON CONFLICT(target_id) DO UPDATE SET native_id=excluded.native_id")
            .bind(target.encode()).bind(id(1).encode()).bind(child).bind(id(2).encode())
            .execute(storage.pool_for_test()).await.unwrap();
    }
    let release = ChildReadRelease(area.join("release-child-root-resume"));
    let first_proof = service
        .prepare_native_read(&id(10), id(8), ReadReason::FirstOpen)
        .await
        .unwrap();
    let second_proof = service
        .prepare_native_read(&id(10), id(5), ReadReason::FirstOpen)
        .await
        .unwrap();
    let first = native
        .acquire_read_source(first_proof, ReadReason::FirstOpen)
        .unwrap();
    let second = native
        .acquire_read_source(second_proof, ReadReason::FirstOpen)
        .unwrap();
    let held = timeout(Duration::from_secs(8), async {
        loop {
            let jobs = source
                .capacity
                .lock()
                .unwrap()
                .values()
                .map(|capacity| capacity.reads.lock().unwrap().len())
                .sum::<usize>();
            if area.join("child-root-resume-held").is_file() && jobs >= 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    if held.is_err() {
        let first_result = timeout(Duration::from_millis(100), first.source_capability())
            .await
            .map(|result| result.map(|_| ()));
        let second_result = timeout(Duration::from_millis(100), second.source_capability())
            .await
            .map(|result| result.map(|_| ()));
        panic!(
            "root resume not reached: first={first_result:?}, second={second_result:?}, requests={:?}, sources={}",
            std::fs::read_to_string(area.join("child-read-requests")),
            native.core.sources.lock().unwrap().len()
        );
    }
    assert_eq!(
        std::fs::read_to_string(area.join("child-read-requests")).unwrap(),
        "synthetic-native-A\n"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM node_session_capacity WHERE released=0")
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
    assert_eq!(
        count, 1,
        "both child installations own one root reservation before response"
    );
    drop(release);
    let (first, second) = tokio::join!(first.source_capability(), second.source_capability());
    if scenario == "child-auto-read-failure" {
        assert!(matches!(
            first,
            Err(crate::domain::shared_view::ViewError::NativeFailure(
                DispatchError::ProtocolRejected
            ))
        ));
        assert!(matches!(
            second,
            Err(crate::domain::shared_view::ViewError::NativeFailure(
                DispatchError::ProtocolRejected
            ))
        ));
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM node_session_capacity WHERE released=0")
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
        assert_eq!(
            count, 1,
            "failed resume keeps original capacity responsibility"
        );
        return;
    }
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.native().native_id(), "synthetic-child-A");
    assert_eq!(second.native().native_id(), "synthetic-child-B");
    assert!(Arc::ptr_eq(&first.capacity, &second.capacity));
    assert_eq!(
        first.configuration_binding(),
        second.configuration_binding()
    );
    assert_eq!(first.pre_source.metadata.config_identity, id(6));
    assert_eq!(
        native.core.sources.lock().unwrap().len(),
        1,
        "second child's different launch configuration follows the original root owner"
    );
    assert_eq!(first.capacity.pre.native.native_id(), "synthetic-native-A");
    assert!(
        source.targets.lock().unwrap().is_empty(),
        "child read does not invent root or child queue resource"
    );
    let attachment = first.source().views().unwrap();
    let views = attachment.views().unwrap();
    for capability in [&first, &second] {
        let snapshot = timeout(Duration::from_secs(8), async {
            loop {
                let changed = views.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if let Ok(target) = views.core.target(capability.native()) {
                    let snapshot = target.snapshot.read().unwrap().clone();
                    if snapshot.projection.records.len() > 0 {
                        break snapshot;
                    }
                }
                changed.await;
            }
        })
        .await
        .unwrap();
        let body = snapshot.projection.records.get(0).unwrap();
        assert!(
            std::str::from_utf8(body.raw.bytes())
                .unwrap()
                .contains(&format!(
                    "automatically delivered {}",
                    capability.native().native_id()
                ))
        );
        let request = attachment
            .read_control()
            .unwrap()
            .request(capability.native(), ReadReason::FirstOpen)
            .unwrap();
        timeout(Duration::from_secs(8), async {
            let mut changes = request.subscribe();
            loop {
                let current = changes.borrow_and_update().clone();
                if current.read_mode() == crate::domain::shared_view::ReadMode::CurrentOnly {
                    break;
                }
                changes.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(area.join("child-read-requests")).unwrap(),
        "synthetic-native-A\n"
    );
    if scenario == "child-auto-read-join-failure" {
        attachment
            .spawn_target_task_for_test(
                first.native(),
                Box::pin(async {
                    panic!("controlled view-task JoinError for capacity ownership verification");
                }),
            )
            .unwrap();
        assert_eq!(
            native
                .core
                .join_capacity_reads(&source, &first.capacity)
                .await,
            Err(DispatchError::ResourceUnconfirmed),
            "a failed actual JoinHandle cannot prove root read convergence"
        );
        assert_eq!(native.inspect_capacity().await.unwrap().occupied, 1);
        return;
    }
    let (release_task, held_task) = tokio::sync::oneshot::channel::<()>();
    attachment
        .spawn_target_task_for_test(
            first.native(),
            Box::pin(async move {
                let _ = held_task.await;
                Ok(())
            }),
        )
        .unwrap();
    {
        let _gate = source.pre.authority.gate.lock().unwrap();
        first
            .capacity
            .pre
            .authority
            .closed
            .store(true, Ordering::Release);
        source.view_attachment.seal_target(first.native());
    }
    assert!(
        timeout(
            Duration::from_millis(25),
            native.core.join_capacity_reads(&source, &first.capacity)
        )
        .await
        .is_err(),
        "last actual view task must keep the original root join pending"
    );
    assert!(
        !views
            .core
            .target(second.native())
            .unwrap()
            .send
            .current()
            .unwrap(),
        "closing the root must also seal its other child before joining all reads"
    );
    assert_eq!(native.inspect_capacity().await.unwrap().occupied, 1);
    assert!(matches!(
        attachment.spawn_target_task_for_test(first.native(), Box::pin(async { Ok(()) })),
        Err(crate::domain::shared_view::ViewError::SourceReleased)
    ));
    let _ = release_task.send(());
    native
        .core
        .join_capacity_reads(&source, &first.capacity)
        .await
        .unwrap();
    assert_eq!(
        first.require_current(),
        Err(crate::domain::shared_view::ViewError::SourceReleased)
    );
    assert_eq!(
        second.require_current(),
        Err(crate::domain::shared_view::ViewError::SourceReleased)
    );
}

async fn seed_child_read_tree(pool: &sqlx::SqlitePool, source: &PreSourceProbe) {
    let meta = source.metadata();
    sqlx::query("INSERT INTO node_catalog_commands(request_id,actor_id,register_auth_generation,action,state,registered_at_ms,intent_cipher,intent_digest) VALUES(?, ?,1,'harness.install','registered',0,X'01',zeroblob(32))")
        .bind(id(50).encode()).bind(id(10).encode()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO node_launch_profiles(profile_id,revision,harness_id,source_id,payload_format,protected_payload,payload_digest,created_by_request_id,created_at_ms) VALUES(?,X'0000000000000001',?,?,1,X'01',zeroblob(32),?,0)")
        .bind(id(6).encode()).bind(id(3).encode()).bind(id(1).encode()).bind(id(50).encode()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO node_source_session_registrations(operation_id,action,actor_id,register_auth_generation,source_id,harness_id,profile_id,profile_revision,workspace_id,launch_path,intent_digest,registered_at_ms) VALUES(?,'create',?,1,?,?,?,X'0000000000000001',?,'fixture',zeroblob(32),0)")
        .bind(id(51).encode()).bind(id(10).encode()).bind(id(1).encode()).bind(id(3).encode()).bind(id(6).encode()).bind(id(2).encode()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO node_source_session_operations(operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,operation_state,outcome,source_instance_id,connection_epoch,updated_at_ms) VALUES(?,'create',?,1,0,'owned','pending',?,?,0)")
        .bind(id(51).encode()).bind(meta.runtime.0.encode()).bind(meta.instance_id.encode()).bind(meta.connection_epoch.encode()).execute(pool).await.unwrap();
    sqlx::query("UPDATE node_source_session_operations SET protocol_version=2,rpc_id_kind='string',rpc_id_bytes=X'31',frame_length=X'0000000000000001',accepted_length=X'0000000000000001',transport_state='written',transport_recorded_at_ms=1,response_kind='created',native_session_id='synthetic-native-A',response_sequence=X'0000000000000001',response_receipt=X'0000000000000001',response_evidence=zeroblob(32),result_bytes=X'0000000000000001',ack_state='confirmed',operation_state='terminal',outcome='succeeded',terminal_at_ms=2 WHERE operation_id=?")
        .bind(id(51).encode()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO node_session_capacity VALUES(?,?,'synthetic-native-A',?,?,?,1,1,1)")
        .bind(id(51).encode())
        .bind(id(1).encode())
        .bind(meta.instance_id.encode())
        .bind(meta.runtime.0.encode())
        .bind(id(51).encode())
        .execute(pool)
        .await
        .unwrap();
    for child in ["synthetic-child-A", "synthetic-child-B"] {
        sqlx::query("INSERT INTO node_session_parents VALUES(?,?,'synthetic-native-A',?,?,?,X'0000000000000001',X'0000000000000001',zeroblob(32),0)")
            .bind(id(1).encode()).bind(child).bind(meta.instance_id.encode()).bind(meta.runtime.0.encode()).bind(meta.connection_epoch.encode()).execute(pool).await.unwrap();
    }
}

#[path = "runtime_child_join_acceptance.rs"]
mod runtime_child_join_acceptance;

#[path = "capacity_registration_race_acceptance.rs"]
mod capacity_registration_race_acceptance;
