//! Independent source/tree integration over real runtime task inventory + SQLite.
//! Required Created metadata is a controlled development fixture, not a wire/Harness success.
use super::*;
use crate::application::native_dispatch as n;
use crate::application::native_dispatch::SessionTreeRepository;
use crate::domain::shared_view::ViewError;
use std::sync::Mutex;

fn owned_plan(
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
            executable: directory.join("never-launched-independent-fixture.exe"),
            arguments: vec![],
            cwd: directory.to_path_buf(),
            environment: vec![],
        }),
        setups: vec![],
    })
}
async fn strict_original_root(
    storage: &crate::storage::Storage,
    source: &Arc<n::PreSourceProbe>,
) -> n::CapacityRecord {
    let p = storage.pool_for_test();
    let m = source.metadata();
    sqlx::query(
        "INSERT INTO harnesses VALUES(?,'independent tree fixture','not-launched','[]',NULL)",
    )
    .bind(id(3).encode())
    .execute(p)
    .await
    .unwrap();
    sqlx::query("INSERT INTO node_catalog_commands(request_id,actor_id,register_auth_generation,action,state,registered_at_ms,intent_cipher,intent_digest) VALUES(?,?,1,'harness.install','registered',0,X'01',zeroblob(32))")
        .bind(id(81).encode()).bind(id(10).encode()).execute(p).await.unwrap();
    sqlx::query("INSERT INTO node_launch_profiles(profile_id,revision,harness_id,source_id,payload_format,protected_payload,payload_digest,created_by_request_id,created_at_ms) VALUES(?,X'0000000000000001',?,?,1,X'01',zeroblob(32),?,0)")
        .bind(id(6).encode()).bind(id(3).encode()).bind(id(1).encode()).bind(id(81).encode()).execute(p).await.unwrap();
    sqlx::query("INSERT INTO node_source_session_registrations(operation_id,action,actor_id,register_auth_generation,source_id,harness_id,profile_id,profile_revision,workspace_id,launch_path,intent_digest,registered_at_ms) VALUES(?,'create',?,1,?,?,?,X'0000000000000001',?,'fixture',zeroblob(32),0)")
        .bind(id(91).encode()).bind(id(10).encode()).bind(id(1).encode()).bind(id(3).encode()).bind(id(6).encode()).bind(id(2).encode()).execute(p).await.unwrap();
    sqlx::query("INSERT INTO node_source_session_operations(operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,operation_state,outcome,source_instance_id,connection_epoch,updated_at_ms) VALUES(?,'create',?,1,0,'owned','pending',?,?,0)")
        .bind(id(91).encode()).bind(m.runtime.0.encode()).bind(m.instance_id.encode()).bind(m.connection_epoch.encode()).execute(p).await.unwrap();
    let repository = storage.request_registry().native_dispatch_repository();
    let deadline = crate::application::request_registry::RegistryDeadline::new(
        n::Instant::now() + Duration::from_secs(10),
    );
    repository
        .reserve_capacity(
            &n::CapacityReservation {
                id: id(91),
                source: source.clone(),
                native: None,
                create_operation: Some(id(91)),
            },
            &deadline,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE node_source_session_operations SET protocol_version=2,rpc_id_kind='string',rpc_id_bytes=?,frame_length=X'0000000000000001',accepted_length=X'0000000000000001',transport_state='written',transport_recorded_at_ms=1,response_kind='created',native_session_id='independent-root',response_sequence=X'0000000000000001',response_receipt=X'0000000000000001',response_evidence=zeroblob(32),result_bytes=X'0000000000000001',ack_state='confirmed',operation_state='terminal',outcome='succeeded',terminal_at_ms=2 WHERE operation_id=?")
        .bind(id(91).encode().into_bytes()).bind(id(91).encode()).execute(p).await.unwrap();
    repository
        .bind_created_capacity(
            id(91),
            &NativeSessionKey::from_config(Some(id(1)), "independent-root").unwrap(),
            source,
            &deadline,
        )
        .await
        .unwrap()
}

struct Display {
    seals: Mutex<Vec<NativeSessionKey>>,
    restores: Mutex<Vec<NativeSessionKey>>,
}
impl n::NativeSourceViewOwner for Display {
    fn bind_target(&self, _: &NativeSessionKey) -> Result<(), ViewError> {
        Err(ViewError::InvalidStage)
    }
    fn seal(&self) -> Result<(), ViewError> {
        Ok(())
    }
    fn seal_target(&self, target: &NativeSessionKey) -> Result<(), ViewError> {
        self.seals.lock().unwrap().push(target.clone());
        Ok(())
    }
    fn restore_target(&self, target: &NativeSessionKey) -> Result<(), ViewError> {
        self.restores.lock().unwrap().push(target.clone());
        Ok(())
    }
    fn join_stopped(&self) -> n::BoxFuture<'_, n::ViewAttachmentClose> {
        Box::pin(async {
            n::ViewAttachmentClose {
                workers: 0,
                readers: 0,
                cleanup: 0,
                native_reads: 0,
                failure: None,
            }
        })
    }
}
struct Factory;
impl n::NativeSourceViewFactory<AcpNativeAdapter> for Factory {
    fn attach(
        &self,
        _: n::ViewSourceContext<AcpNativeAdapter>,
        _: n::AttachmentBuildGuard,
    ) -> Result<(), ViewError> {
        Err(ViewError::CaptureUnavailable)
    }
}

/// OT-009: actual source pending inventory survives failed necessary relationship storage.
#[tokio::test]
async fn ot009_source_parent_save_failure_keeps_required_until_exact_proof_retry() {
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
    let source = core.ensure_source_slot(owned_plan(&directory)).unwrap();
    core.repository
        .reserve_source(&source.pre, &core.deadline().unwrap())
        .await
        .unwrap();
    source.recorded.store(true, Ordering::Release);
    let record = strict_original_root(&storage, &source.pre).await;
    let root = NativeSessionKey::from_config(Some(id(1)), "independent-root").unwrap();
    let resource = core.capacity_resource(&source, &root).unwrap();
    *resource.record.lock().unwrap() = Some(record.clone());
    core.restore_tree_members(&source, &root).await.unwrap();
    sqlx::query("CREATE TRIGGER independent_pending_tree_fail BEFORE INSERT ON node_session_parents BEGIN SELECT RAISE(ABORT,'independent required relationship failure'); END")
        .execute(storage.pool_for_test()).await.unwrap();
    core.accept_tree_observation(&source, "independent-root", "child", 17, 23)
        .unwrap();
    assert_eq!(
        source.pre.authority.required.load(Ordering::Acquire),
        1,
        "protection installed synchronously before owned SQL task"
    );
    assert!(
        source
            .tasks
            .join(n::Instant::now() + Duration::from_secs(10))
            .await
    );
    assert_eq!(
        source.pre.authority.required.load(Ordering::Acquire),
        1,
        "failed storage cannot clear required"
    );
    assert_eq!(source.tree_observations.lock().unwrap().len(), 1);
    let child = NativeSessionKey::from_config(Some(id(1)), "child").unwrap();
    assert_eq!(
        core.repository
            .resolve_session_tree(&child, &core.deadline().unwrap())
            .await,
        Err(DispatchError::SessionTreeUnverified)
    );
    assert_eq!(
        core.repository
            .inspect_capacity(&core.deadline().unwrap())
            .await
            .unwrap()
            .occupied,
        1
    );
    sqlx::query("DROP TRIGGER independent_pending_tree_fail")
        .execute(storage.pool_for_test())
        .await
        .unwrap();
    core.reconcile_tree_observations(&source).await.unwrap();
    assert_eq!(source.pre.authority.required.load(Ordering::Acquire), 0);
    let proof: (Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT sequence,receipt FROM node_session_parents")
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
    assert_eq!(
        proof,
        (17_u64.to_be_bytes().to_vec(), 23_u64.to_be_bytes().to_vec())
    );
    assert_eq!(
        core.repository
            .resolve_session_tree(&child, &core.deadline().unwrap())
            .await
            .unwrap()
            .root,
        Some(root)
    );
    assert_eq!(resource.record.lock().unwrap().as_ref(), Some(&record));
    let shutdown = runtime
        .shutdown(n::Instant::now() + Duration::from_secs(10))
        .await;
    assert!(
        shutdown.joined && shutdown.metadata_confirmed,
        "{shutdown:?}"
    );
    drop(service);
    drop(runtime);
    storage.close().await;
    scope.stop_and_observe().await;
    assert!(storage.registry_close_status().is_complete());
}

/// OT-010: root Closed authority seals a newly observed descendant, and never restores it.
#[tokio::test]
async fn ot010_closed_root_late_child_is_sealed_and_original_authority_stays_closed() {
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
    runtime.install_view_factory(Arc::new(Factory)).unwrap();
    let core = &runtime.core;
    let source = core.ensure_source_slot(owned_plan(&directory)).unwrap();
    core.repository
        .reserve_source(&source.pre, &core.deadline().unwrap())
        .await
        .unwrap();
    source.recorded.store(true, Ordering::Release);
    let record = strict_original_root(&storage, &source.pre).await;
    let root = NativeSessionKey::from_config(Some(id(1)), "independent-root").unwrap();
    let resource = core.capacity_resource(&source, &root).unwrap();
    *resource.record.lock().unwrap() = Some(record.clone());
    core.restore_tree_members(&source, &root).await.unwrap();
    let display = Arc::new(Display {
        seals: Mutex::new(vec![]),
        restores: Mutex::new(vec![]),
    });
    let guard = source.view_attachment.begin(source.pre.clone()).unwrap();
    guard.adopt_partial(display.clone()).unwrap();
    guard.publish().unwrap();
    // Controlled fixture cut models an already closed original root; no source owner is invented.
    resource.pre.authority.closed.store(true, Ordering::Release);
    core.accept_tree_observation(&source, "independent-root", "late-child", 31, 47)
        .unwrap();
    let child = NativeSessionKey::from_config(Some(id(1)), "late-child").unwrap();
    assert_eq!(
        *display.seals.lock().unwrap(),
        vec![child.clone()],
        "late relation must seal child synchronously"
    );
    assert!(display.restores.lock().unwrap().is_empty());
    assert!(
        source
            .tasks
            .join(n::Instant::now() + Duration::from_secs(10))
            .await
    );
    assert!(resource.pre.authority.closed.load(Ordering::Acquire));
    let restored = std::sync::atomic::AtomicBool::new(false);
    assert_eq!(
        n::capacity::restore_display(&source.pre, &resource.pre, || {
            restored.store(true, Ordering::Release);
            Ok(())
        }),
        Err(ViewError::SourceReleased),
        "late relation cannot reauthorize the original root display"
    );
    assert!(!restored.load(Ordering::Acquire));
    assert_eq!(source.capacity.lock().unwrap().len(), 1);
    assert_eq!(resource.record.lock().unwrap().as_ref(), Some(&record));
    assert_eq!(
        core.repository
            .inspect_capacity(&core.deadline().unwrap())
            .await
            .unwrap()
            .occupied,
        1,
        "relation persistence cannot refund original allocation"
    );
    core.accept_tree_observation(&source, "independent-root", "late-child", 32, 48)
        .unwrap();
    assert_eq!(display.seals.lock().unwrap().len(), 2);
    assert!(display.restores.lock().unwrap().is_empty());
    assert_eq!(
        source.tree_observations.lock().unwrap().len(),
        1,
        "duplicate retains exact original inventory"
    );
    assert!(resource.pre.authority.closed.load(Ordering::Acquire));
    let shutdown = runtime
        .shutdown(n::Instant::now() + Duration::from_secs(10))
        .await;
    assert!(
        shutdown.joined && shutdown.metadata_confirmed,
        "{shutdown:?}"
    );
    drop(service);
    drop(runtime);
    storage.close().await;
    scope.stop_and_observe().await;
    assert!(storage.registry_close_status().is_complete());
}
