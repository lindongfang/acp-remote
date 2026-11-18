//! Controlled ACP child + real SQLite. This does not certify a production Harness.
use super::*;
use crate::{
    adapters::acp::{native_dispatch::AcpNativeAdapter, *},
    application::{
        native_dispatch::{
            CapacityRepository, ConfigurationKey, NativeLifecyclePort, ProtocolVersion,
            SourceInitializationPlan,
            dev_tests::{Configuration, Retain, Validator, options},
        },
        request_registry::{QueueAction, RegistryError, RequestAuthority, SystemNodeClock},
    },
    domain::{ActorContext, native::NativeSessionKey},
    platform::native_source::{WindowsLaunchPlan, WindowsSourceLauncher},
    storage::{NodeMeta, SCHEMA_VERSION},
};
use std::{path::PathBuf, task::Context, time::Duration};

const fn id(value: u8) -> EntityId {
    EntityId::from_bytes([value; 16])
}

#[test]
fn automatic_permissions_protect_unknown_and_allow_written_or_retired() {
    for (written, retired, expected) in [
        (false, false, false),
        (true, false, true),
        (false, true, true),
        (true, true, true),
    ] {
        assert_eq!(
            permission_state_settled(super::super::environment::PermissionState {
                obligation: Some(id(1)),
                claimed: Some(id(2)),
                written,
                retired,
            }),
            expected
        );
    }
}

struct Authority;
impl RequestAuthority for Authority {
    type Actor = EntityId;
    fn authorize(
        &self,
        actor: &EntityId,
        _: EntityId,
        _: QueueAction,
    ) -> impl Future<Output = Result<ActorContext, RegistryError>> {
        std::future::ready(Ok(ActorContext {
            client_id: *actor,
            auth_generation: 1,
        }))
    }
    async fn authorize_read(
        &self,
        actor: &EntityId,
        target: EntityId,
    ) -> Result<ActorContext, RegistryError> {
        self.authorize(actor, target, QueueAction::Submit).await
    }
    fn resolve_target(
        &self,
        _: EntityId,
    ) -> impl Future<Output = Result<NativeSessionKey, RegistryError>> {
        std::future::ready(Ok(NativeSessionKey::from_config(
            Some(id(1)),
            "synthetic-native-A",
        )
        .unwrap()))
    }
}

struct Io {
    accepted: Option<usize>,
    calls: usize,
}

#[tokio::test]
async fn automatic_scan_failed_registration_never_reports_enabled() {
    use crate::application::{
        native_dispatch::{NativeEnvironment, NativeRuntimeHandle},
        queue_owner::QueueService,
        request_registry::NoAudit,
    };
    use crate::domain::native::RuntimeEpoch;

    let (storage, scope, _) = storage_fixture().await;
    let repository = storage.request_registry();
    let (environment, _changes) = NativeEnvironment::new();
    let claim = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let runtime_epoch = RuntimeEpoch(id(250));
    let (service, driver) = QueueService::new(
        Authority,
        Validator,
        repository.clone(),
        repository.owner_scope(),
        SystemNodeClock,
        runtime_epoch,
        environment.clone(),
        Arc::new(NoAudit),
        claim.clone(),
        Arc::default(),
    );
    let core = NativeCoordinator::create(
        Arc::new(AcpNativeAdapter::default()),
        Arc::new(WindowsSourceLauncher),
        Arc::new(storage.native_configuration_provider(Arc::new(Configuration))),
        repository.native_dispatch_repository(),
        driver,
        Arc::new(Retain),
        environment,
        service.native_clock(),
        runtime_epoch,
        claim,
        options(),
    );
    core.status.store(1, Ordering::Release);
    core.tasks.seal();
    let runtime = NativeRuntimeHandle { core: core.clone() };
    let result = core.start_capacity_scan();
    let enabled = runtime.automatic_recycling_enabled();
    core.fail_all(DispatchError::NodeStopping);
    drop(runtime);
    drop(core);
    drop(service);
    drop(repository);
    storage.close().await;
    scope.stop_and_observe().await;
    assert_eq!(result, Err(DispatchError::NodeStopping));
    assert!(
        !enabled,
        "a failed task registration cannot publish an enabled scan"
    );
}

async fn seed_synthetic_root(
    storage: &crate::storage::Storage,
    source: &Arc<super::super::PreSourceProbe>,
) -> CapacityRecord {
    let metadata = &source.metadata;
    let repository = storage.request_registry().native_dispatch_repository();
    let deadline = crate::application::request_registry::RegistryDeadline::new(
        super::super::Instant::now() + Duration::from_secs(5),
    );
    let pool = storage.pool_for_test();
    sqlx::query("INSERT INTO harnesses VALUES(?,'automatic fixture','fixture','[]',NULL)")
        .bind(id(3).encode())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO node_catalog_commands(request_id,actor_id,register_auth_generation,action,state,registered_at_ms,intent_cipher,intent_digest) VALUES(?,?,1,'harness.install','registered',0,X'01',zeroblob(32))")
        .bind(id(80).encode()).bind(id(10).encode()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO node_launch_profiles(profile_id,revision,harness_id,source_id,payload_format,protected_payload,payload_digest,created_by_request_id,created_at_ms) VALUES(?,X'0000000000000001',?,?,1,X'01',zeroblob(32),?,0)")
        .bind(id(6).encode()).bind(id(3).encode()).bind(id(1).encode()).bind(id(80).encode())
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO node_source_session_registrations(operation_id,action,actor_id,register_auth_generation,source_id,harness_id,profile_id,profile_revision,workspace_id,launch_path,intent_digest,registered_at_ms) VALUES(?,'create',?,1,?,?,?,X'0000000000000001',?,'fixture',zeroblob(32),0)")
        .bind(id(90).encode()).bind(id(10).encode()).bind(id(1).encode()).bind(id(3).encode())
        .bind(id(6).encode()).bind(id(2).encode()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO node_source_session_operations(operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,operation_state,outcome,source_instance_id,connection_epoch,updated_at_ms) VALUES(?,'create',?,1,0,'owned','pending',?,?,0)")
        .bind(id(90).encode()).bind(metadata.runtime.0.encode()).bind(metadata.instance_id.encode())
        .bind(metadata.connection_epoch.encode()).execute(pool).await.unwrap();
    repository
        .reserve_capacity(
            &super::super::CapacityReservation {
                id: id(90),
                source: source.clone(),
                native: None,
                create_operation: Some(id(90)),
            },
            &deadline,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE node_source_session_operations SET protocol_version=2,rpc_id_kind='string',rpc_id_bytes=?,frame_length=X'0000000000000001',accepted_length=X'0000000000000001',transport_state='written',transport_recorded_at_ms=1,response_kind='created',native_session_id='synthetic-native-A',response_sequence=X'0000000000000001',response_receipt=X'0000000000000001',response_evidence=zeroblob(32),result_bytes=X'0000000000000001',ack_state='confirmed',operation_state='terminal',outcome='succeeded',terminal_at_ms=2 WHERE operation_id=?")
        .bind(id(90).encode().into_bytes()).bind(id(90).encode()).execute(pool).await.unwrap();
    repository
        .bind_created_capacity(
            id(90),
            &NativeSessionKey::from_config(Some(id(1)), "synthetic-native-A").unwrap(),
            source,
            &deadline,
        )
        .await
        .unwrap()
}
impl DispatchPollIo for Io {
    fn poll(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<Result<DispatchIoProgress, super::super::DispatchIoError>> {
        self.calls += 1;
        self.accepted.map_or(Poll::Pending, |count| {
            Poll::Ready(Ok(DispatchIoProgress::Accepted(count)))
        })
    }
    fn park(&mut self) -> Result<(), super::super::DispatchIoError> {
        Ok(())
    }
}

async fn storage_fixture() -> (
    crate::sqlite_fixture_scope::FixtureStorage,
    crate::sqlite_fixture_scope::SqliteFixtureScope,
    Arc<PathBuf>,
) {
    // Keep all artifacts in the dedicated package workspace, even when a test fails.
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../test-evidence")
        .join(format!("automatic-{}", crate::random::hex(10).unwrap()));
    std::fs::create_dir_all(&directory).unwrap();
    let directory = Arc::new(directory);
    let scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &directory.join("node.sqlite3"),
        directory.clone(),
    );
    let storage = scope.open().await.unwrap();
    storage
        .write_node_meta(&NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "automatic-internal-fixture".into(),
            public_key: vec![1, 2, 3],
            key_file_ref: "synthetic-unused".into(),
        })
        .await
        .unwrap();
    sqlx::query("INSERT INTO clients VALUES(?,?,'automatic dev','trusted',1,0,NULL)")
        .bind(id(10).encode())
        .bind(vec![1_u8])
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_sources VALUES(?,'automatic dev')")
        .bind(id(1).encode())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces VALUES(?,?)")
        .bind(id(2).encode())
        .bind(directory.to_str().unwrap())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
        .bind(id(4).encode())
        .bind(id(1).encode())
        .bind("synthetic-native-A")
        .bind(id(2).encode())
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    (storage, scope, directory)
}

fn plan(
    directory: &std::path::Path,
) -> Arc<SourceInitializationPlan<WindowsLaunchPlan, SetupSnapshot>> {
    Arc::new(SourceInitializationPlan {
        source_id: id(1),
        workspace: id(2),
        harness: id(3),
        configuration: ConfigurationKey {
            identity: id(6),
            revision: 1,
        },
        launch: Arc::new(WindowsLaunchPlan {
            executable: crate::test_support::native_dispatch_dev_child(&[
                "NATIVE_DISPATCH_DEV_CHILD",
            ]),
            arguments: vec!["source-create-replay-state".into()],
            cwd: directory.to_path_buf(),
            environment: vec![],
        }),
        setups: vec![(
            ProtocolVersion::V2,
            SetupSnapshot {
                wire: Version::V2,
                source: ConfigSourceStamp {
                    reference: Arc::from(id(6).encode()),
                    revision: 1,
                },
                cwd: Known::Known(Arc::from(directory.to_str().unwrap())),
                mcp_servers: Known::Known(vec![]),
                additional_directories: Known::Known(vec![]),
                extra: ExtensionFields::empty().unwrap(),
            },
        )],
    })
}

async fn pending_create(
    storage: &crate::storage::Storage,
    source: &Arc<super::super::PreSourceProbe>,
    operation: EntityId,
) -> super::super::CapacityReservation {
    let metadata = &source.metadata;
    sqlx::query("INSERT INTO node_source_session_registrations(operation_id,action,actor_id,register_auth_generation,source_id,harness_id,profile_id,profile_revision,workspace_id,launch_path,intent_digest,registered_at_ms) VALUES(?,'create',?,1,?,?,?,X'0000000000000001',?,'fixture',zeroblob(32),0)")
        .bind(operation.encode()).bind(id(10).encode()).bind(id(1).encode()).bind(id(3).encode())
        .bind(id(6).encode()).bind(id(2).encode()).execute(storage.pool_for_test()).await.unwrap();
    sqlx::query("INSERT INTO node_source_session_operations(operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,operation_state,outcome,source_instance_id,connection_epoch,updated_at_ms) VALUES(?,'create',?,1,0,'owned','pending',?,?,0)")
        .bind(operation.encode()).bind(metadata.runtime.0.encode()).bind(metadata.instance_id.encode())
        .bind(metadata.connection_epoch.encode()).execute(storage.pool_for_test()).await.unwrap();
    super::super::CapacityReservation {
        id: operation,
        source: source.clone(),
        native: None,
        create_operation: Some(operation),
    }
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep full admission, one original release and owned scan shutdown assertions together"
)]
async fn automatic_admission_full_closes_one_tree_and_scan_is_joined() {
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
    let capacity = core.capacity_resource(&source, &native).unwrap();
    *capacity.record.lock().unwrap() = Some(created);
    core.reserve_native_capacity(&source, &capacity)
        .await
        .unwrap();
    source.capacity_activity.lock().unwrap().created(
        native.native_id(),
        ProtocolVersion::V2,
        Instant::now(),
    );
    for number in 100..149 {
        let reservation = pending_create(&storage, &source.pre, id(number)).await;
        core.repository
            .reserve_capacity(&reservation, &core.deadline().unwrap())
            .await
            .unwrap();
    }
    let request = pending_create(&storage, &source.pre, id(149)).await;
    assert!(
        runtime.automatic_recycling_enabled(),
        "normal runtime startup must register its owned capacity scan"
    );
    assert_eq!(core.start_capacity_scan(), Err(DispatchError::Conflict));
    source
        .pre
        .authority
        .quarantined
        .store(true, Ordering::Release);
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), core.admit_capacity(&request))
            .await
            .unwrap(),
        Err(DispatchError::SessionCapacityFull)
    ));
    assert!(capacity.automatic.lock().unwrap().is_none());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM node_session_automatic_releases")
        .fetch_one(storage.pool_for_test())
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "no eligible candidate must not create a Close intent"
    );
    source
        .pre
        .authority
        .quarantined
        .store(false, Ordering::Release);
    let admitted = core.admit_capacity(&request).await.unwrap();
    assert_eq!(
        admitted.id, request.id,
        "retry retains the original admission"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM node_session_automatic_releases WHERE phase='finished' AND reason='capacity_pressure'")
        .fetch_one(storage.pool_for_test()).await.unwrap();
    assert_eq!(count, 1, "one pressure request closes only one candidate");
    assert_eq!(
        core.repository
            .inspect_capacity(&core.deadline().unwrap())
            .await
            .unwrap()
            .occupied,
        50
    );
    let rejected = pending_create(&storage, &source.pre, id(150)).await;
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), core.admit_capacity(&rejected))
            .await
            .unwrap(),
        Err(DispatchError::SessionCapacityFull)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM node_session_automatic_releases")
        .fetch_one(storage.pool_for_test())
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "finished ticket must not count as another returned slot"
    );
    // These capacity fillers never owned a native transport ticket. Supply the
    // original durable pre-arm refusal before asking shutdown to reconcile them.
    for number in 100..=150 {
        sqlx::query("UPDATE node_source_session_operations SET operation_state='terminal',outcome='refused',failure_class='metadata',terminal_at_ms=1 WHERE operation_id=?")
            .bind(id(number).encode()).execute(storage.pool_for_test()).await.unwrap();
        if number < 150 {
            core.repository
                .abandon_capacity(id(number), &source.pre, &core.deadline().unwrap())
                .await
                .unwrap();
        }
    }
    let report = runtime
        .shutdown(super::super::Instant::now() + Duration::from_secs(5))
        .await;
    assert!(report.joined && report.metadata_confirmed, "{report:?}");
    assert!(!runtime.automatic_recycling_enabled());
    drop(service);
    drop(runtime);
    storage.close().await;
    scope.stop_and_observe().await;
    eprintln!(
        "real SQL Full admission and owned scan shutdown; artifacts={}",
        directory.display()
    );
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "One original boundary is followed from Pending through protection changes and partial write freeze"
)]
async fn automatic_boundary_pending_rechecks_and_first_accept_keeps_original_freeze() {
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
    let capacity = core.capacity_resource(&source, &native).unwrap();
    *capacity.record.lock().unwrap() = Some(created);
    core.reserve_native_capacity(&source, &capacity)
        .await
        .unwrap();
    source.capacity_activity.lock().unwrap().created(
        native.native_id(),
        ProtocolVersion::V2,
        Instant::now(),
    );
    let original = capacity.record.lock().unwrap().clone().unwrap();
    let tree = core.automatic_tree(&source, &capacity).unwrap();
    let before = Instant::now();
    let old_candidate = with_candidate(
        &*core.protocol,
        &source,
        &capacity,
        &tree,
        &original,
        before,
        0,
        Ok,
    )
    .unwrap()
    .unwrap();
    let later = before + Duration::from_millis(1);
    let unchanged = with_candidate(
        &*core.protocol,
        &source,
        &capacity,
        &tree,
        &original,
        later,
        2,
        Ok,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        unchanged, old_candidate,
        "synthetic queue revision change without real activity must not reset idle time"
    );
    // Continue real-time boundary checks without advancing its monotonic clock into the future.
    while Instant::now() < later {
        std::thread::yield_now();
    }
    // Select using the real queue revision after the synthetic revision check.
    let selected = core
        .automatic_candidate(&source, &capacity, Instant::now())
        .unwrap()
        .unwrap();
    let queue = core.queue.automatic_close_guard(&native).unwrap();
    let wire_scope = core.protocol.source_scope(source.source.get().unwrap());
    let mut boundary = AutomaticBoundary {
        protocol: Arc::clone(&core.protocol),
        source: Arc::downgrade(&source),
        capacity: Arc::downgrade(&capacity),
        tree,
        original,
        operation: id(91),
        reason: AutomaticReleaseReason::CapacityPressure,
        pressure_selection: Some(core.automatic_pressure_selection(&source, &capacity, selected)),
        queue: queue.clone(),
        scope: wire_scope.clone(),
        rpc: super::super::RpcScalar::String(Arc::from("boundary-test")),
    };
    let identity = LifecycleWriteIdentity {
        scope: wire_scope,
        rpc: boundary.rpc.clone(),
        kind: LifecycleKind::Close,
    };
    let mut context = Context::from_waker(std::task::Waker::noop());
    let first = DispatchWriteStep::Chunk {
        offset: 0,
        total: 2,
    };
    let mut pending = Io {
        accepted: None,
        calls: 0,
    };
    boundary
        .check_and_poll(&identity, first, &mut context, &mut pending)
        .unwrap();
    assert_eq!(pending.calls, 1);
    assert!(!capacity.pre.authority.releasing.load(Ordering::Acquire));
    {
        let mut activity = source.capacity_activity.lock().unwrap();
        activity.associate(native.native_id(), "child").unwrap();
        activity.created("child", ProtocolVersion::V2, Instant::now());
    }
    assert!(
        boundary
            .check_and_poll(&identity, first, &mut context, &mut pending)
            .is_err()
    );
    assert_eq!(
        pending.calls, 1,
        "membership change must invalidate the original first-byte snapshot"
    );
    let tree = core.automatic_tree(&source, &capacity).unwrap();
    boundary.tree = tree.clone();
    boundary.queue = tree.queue.clone();
    let queue = tree.queue.clone();
    let child = NativeSessionKey::from_config(Some(id(1)), "child").unwrap();
    let child_guard = core.queue.automatic_close_guard(&child).unwrap();
    let _ = child_guard
        .poll_idle(id(96), &mut || {
            Ok(Poll::Ready(Ok(DispatchIoProgress::Accepted(1))))
        })
        .unwrap();
    assert!(
        boundary
            .check_and_poll(&identity, first, &mut context, &mut pending)
            .is_err()
    );
    assert_eq!(
        pending.calls, 1,
        "any original child owner protects the entire tree"
    );
    assert!(child_guard.finish(id(96)).unwrap());
    let child_state = |foreground| super::super::CapacityObservation {
        session: Arc::from("child"),
        origin: super::super::ObservationOrigin::Live,
        freshness: super::super::CapacityFreshness::Current,
        update: super::super::CapacityUpdate::State(super::super::CapacityState {
            foreground,
            background: None,
        }),
    };
    source
        .capacity_activity
        .lock()
        .unwrap()
        .observe(&child_state(Coverage::Active), Instant::now())
        .unwrap();
    assert!(
        boundary
            .check_and_poll(&identity, first, &mut context, &mut pending)
            .is_err()
    );
    assert_eq!(
        pending.calls, 1,
        "active child protects even with empty native queues"
    );
    source
        .capacity_activity
        .lock()
        .unwrap()
        .observe(&child_state(Coverage::Idle), Instant::now())
        .unwrap();
    source
        .pre
        .authority
        .quarantined
        .store(true, Ordering::Release);
    assert!(
        boundary
            .check_and_poll(&identity, first, &mut context, &mut pending)
            .is_err()
    );
    assert_eq!(pending.calls, 1);
    source
        .pre
        .authority
        .quarantined
        .store(false, Ordering::Release);
    // A recovered candidate needs a fresh selection before another operation
    // may reach first-byte IO.
    let selected = core
        .automatic_candidate(&source, &capacity, Instant::now())
        .unwrap()
        .unwrap();
    boundary.pressure_selection =
        Some(core.automatic_pressure_selection(&source, &capacity, selected));
    let mut accepted = Io {
        accepted: Some(1),
        calls: 0,
    };
    boundary
        .check_and_poll(&identity, first, &mut context, &mut accepted)
        .unwrap();
    assert!(capacity.pre.authority.releasing.load(Ordering::Acquire));
    assert!(
        queue
            .poll_idle(id(92), &mut || panic!("frozen owner entered idle callback"))
            .is_err()
    );
    assert!(!queue.finish(id(92)).unwrap());
    assert!(
        child_guard
            .poll_idle(id(92), &mut || panic!(
                "child was not frozen at first accepted byte"
            ))
            .is_err()
    );
    let root_guard = core.queue.automatic_close_guard(&native).unwrap();
    assert!(
        root_guard
            .poll_idle(id(92), &mut || panic!(
                "root was not frozen at first accepted byte"
            ))
            .is_err()
    );
    source
        .pre
        .authority
        .quarantined
        .store(true, Ordering::Release);
    boundary
        .check_and_poll(
            &identity,
            DispatchWriteStep::Chunk {
                offset: 1,
                total: 2,
            },
            &mut context,
            &mut accepted,
        )
        .unwrap();
    assert_eq!(
        accepted.calls, 2,
        "continuation checks original freeze, not new idle"
    );
    assert!(queue.finish(id(91)).unwrap());
    assert!(
        !child_guard.finish(id(91)).unwrap(),
        "composite finish releases all original members"
    );
    source
        .pre
        .authority
        .quarantined
        .store(false, Ordering::Release);
    capacity
        .pre
        .authority
        .releasing
        .store(false, Ordering::Release);
    let report = runtime
        .shutdown(super::super::Instant::now() + Duration::from_secs(5))
        .await;
    assert!(report.joined && report.metadata_confirmed);
    drop(service);
    drop(runtime);
    storage.close().await;
    scope.stop_and_observe().await;
    eprintln!(
        "boundary actual initialized ACP source; controlled IO Pending/Accepted; artifacts={}",
        directory.display()
    );
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Follow the original close and subsequent generation without replacing their owners"
)]
async fn automatic_real_close_finishes_original_once_and_replaces_closed_generation() {
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
    let capacity = core.capacity_resource(&source, &native).unwrap();
    *capacity.record.lock().unwrap() = Some(created);
    core.reserve_native_capacity(&source, &capacity)
        .await
        .unwrap();
    source.capacity_activity.lock().unwrap().created(
        native.native_id(),
        ProtocolVersion::V2,
        Instant::now(),
    );
    assert!(
        core.automatic_candidate(&source, &capacity, Instant::now())
            .unwrap()
            .is_some()
    );
    assert!(
        !core
            .automatic_release(&source, &capacity, AutomaticReleaseReason::IdleTimeout)
            .await
            .unwrap()
    );
    assert!(
        core.automatic_release(&source, &capacity, AutomaticReleaseReason::CapacityPressure)
            .await
            .unwrap()
    );
    let operation = capacity.automatic.lock().unwrap().clone().unwrap();
    let lifecycle = operation.lifecycle.lock().unwrap().clone().unwrap();
    assert_eq!(
        operation.record.lock().unwrap().as_ref().unwrap().phase,
        AutomaticReleasePhase::Finished
    );
    assert_eq!(retained_reason(&capacity).unwrap(), None);
    assert_eq!(
        operation.record.lock().unwrap().as_ref().unwrap().outcome,
        Some(AutomaticReleaseOutcome::Requested)
    );
    assert_eq!(
        core.repository
            .inspect_capacity(&core.deadline().unwrap())
            .await
            .unwrap()
            .occupied,
        0
    );
    assert!(
        core.automatic_release(&source, &capacity, AutomaticReleaseReason::CapacityPressure)
            .await
            .unwrap()
    );
    assert!(Arc::ptr_eq(
        &lifecycle,
        operation.lifecycle.lock().unwrap().as_ref().unwrap()
    ));
    let replacement = core.capacity_resource(&source, &native).unwrap();
    assert!(!Arc::ptr_eq(&replacement, &capacity));
    assert!(replacement.automatic.lock().unwrap().is_none());
    assert!(
        source
            .capacity_activity
            .lock()
            .unwrap()
            .session(native.native_id())
            .is_none()
    );
    core.reserve_native_capacity(&source, &replacement)
        .await
        .unwrap();
    source.capacity_activity.lock().unwrap().created(
        native.native_id(),
        ProtocolVersion::V2,
        Instant::now(),
    );
    let selected_since = core
        .automatic_candidate(&source, &replacement, Instant::now())
        .unwrap()
        .unwrap();
    let retained = Arc::new(AutomaticOperation {
        id: id(97),
        original: replacement.record.lock().unwrap().clone().unwrap(),
        reason: AutomaticReleaseReason::CapacityPressure,
        pressure_selection: Some(core.automatic_pressure_selection(
            &source,
            &replacement,
            selected_since,
        )),
        queue: core.queue.automatic_close_guard(&native).unwrap(),
        tree: core.automatic_tree(&source, &replacement).unwrap(),
        record: Mutex::new(None),
        lifecycle: Mutex::new(None),
    });
    *replacement.automatic.lock().unwrap() = Some(retained.clone());
    let record = core
        .repository
        .begin_automatic_release(
            &retained.original,
            retained.id,
            retained.reason,
            &core.deadline().unwrap(),
        )
        .await
        .unwrap();
    *retained.record.lock().unwrap() = Some(record);
    let retained_lifecycle = core
        .prepare_automatic_close(
            &source,
            &replacement,
            &retained,
            core.effective_close_deadline(&source).unwrap(),
        )
        .unwrap();
    // Controlled lost outcome: durable Unknown keeps this original ticket; the
    // actual ACP Closed response subsequently permits finishing the same op.
    core.save_automatic_outcome(&retained, AutomaticReleaseOutcome::Unknown)
        .await
        .unwrap();
    assert!(
        core.automatic_release(&source, &replacement, retained.reason)
            .await
            .unwrap()
    );
    assert_eq!(
        retained.record.lock().unwrap().as_ref().unwrap().outcome,
        Some(AutomaticReleaseOutcome::Unknown)
    );
    assert!(Arc::ptr_eq(
        &retained_lifecycle,
        retained.lifecycle.lock().unwrap().as_ref().unwrap()
    ));
    assert_eq!(
        core.repository
            .inspect_capacity(&core.deadline().unwrap())
            .await
            .unwrap()
            .occupied,
        0
    );
    let zero_capacity = core.capacity_resource(&source, &native).unwrap();
    core.reserve_native_capacity(&source, &zero_capacity)
        .await
        .unwrap();
    source.capacity_activity.lock().unwrap().created(
        native.native_id(),
        ProtocolVersion::V2,
        Instant::now(),
    );
    let selected_since = core
        .automatic_candidate(&source, &zero_capacity, Instant::now())
        .unwrap()
        .unwrap();
    let zero = Arc::new(AutomaticOperation {
        id: id(98),
        original: zero_capacity.record.lock().unwrap().clone().unwrap(),
        reason: AutomaticReleaseReason::CapacityPressure,
        pressure_selection: Some(core.automatic_pressure_selection(
            &source,
            &zero_capacity,
            selected_since,
        )),
        queue: core.queue.automatic_close_guard(&native).unwrap(),
        tree: core.automatic_tree(&source, &zero_capacity).unwrap(),
        record: Mutex::new(None),
        lifecycle: Mutex::new(None),
    });
    *zero_capacity.automatic.lock().unwrap() = Some(zero.clone());
    *zero.record.lock().unwrap() = Some(
        core.repository
            .begin_automatic_release(
                &zero.original,
                zero.id,
                zero.reason,
                &core.deadline().unwrap(),
            )
            .await
            .unwrap(),
    );
    let zero_lifecycle = core
        .prepare_automatic_close(
            &source,
            &zero_capacity,
            &zero,
            core.effective_close_deadline(&source).unwrap(),
        )
        .unwrap();
    // Current-thread runtime has not yielded after enqueue. The first writer poll
    // therefore observes this real gate change before performing IO.
    source
        .pre
        .authority
        .quarantined
        .store(true, Ordering::Release);
    assert!(
        !core
            .automatic_release(&source, &zero_capacity, zero.reason)
            .await
            .unwrap()
    );
    assert_eq!(
        zero.record.lock().unwrap().as_ref().unwrap().phase,
        AutomaticReleasePhase::Withdrawn
    );
    assert!(zero_lifecycle.response.lock().unwrap().is_none());
    assert!(
        !zero_capacity
            .pre
            .authority
            .releasing
            .load(Ordering::Acquire)
    );
    assert_eq!(
        core.repository
            .inspect_capacity(&core.deadline().unwrap())
            .await
            .unwrap()
            .occupied,
        1
    );
    source
        .pre
        .authority
        .quarantined
        .store(false, Ordering::Release);
    assert!(
        core.automatic_release(&source, &zero_capacity, zero.reason)
            .await
            .unwrap()
    );
    assert_ne!(
        zero.id,
        zero_capacity.automatic.lock().unwrap().as_ref().unwrap().id
    );
    let report = runtime
        .shutdown(super::super::Instant::now() + Duration::from_secs(5))
        .await;
    assert!(report.joined && report.metadata_confirmed);
    drop(service);
    drop(runtime);
    storage.close().await;
    scope.stop_and_observe().await;
    eprintln!(
        "real SQLite/ACP child close completed once; synthetic native root; artifacts={}",
        directory.display()
    );
}

#[path = "source_tree_acceptance.rs"]
mod independent_acceptance;

#[path = "capacity_automatic_rank_tests.rs"]
mod rank_tests;

#[path = "runtime_child_finish_acceptance.rs"]
mod runtime_child_finish_acceptance;

#[path = "capacity_late_child_race_acceptance.rs"]
mod capacity_late_child_race_acceptance;
