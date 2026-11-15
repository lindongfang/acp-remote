#![allow(
    clippy::single_match,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::*;
use crate::node::tests::{FakeProtector, TestArea, free_address};

async fn actual(operation: &NodeCloseOperation, expected: Actual) {
    let mut rx = operation.actual.subscribe();
    timeout(Duration::from_secs(5), async {
        loop {
            if *rx.borrow_and_update() == expected {
                break;
            }
            rx.changed()
                .await
                .expect("original driver progress remains observable");
        }
    })
    .await
    .expect("original operation reaches expected actual state");
}
fn owner(area: &TestArea) -> NodeProcessOwner {
    NodeProcessOwner::begin_control_only_for_test(
        &Handle::current(),
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
    )
}
fn locked(operation: &NodeCloseOperation, area: &TestArea) {
    assert!(lock(&operation.resources).directory.is_some());
    assert!(matches!(
        DataDirectory::acquire(&area.data_dir()),
        Err(AppError::DataDirectoryBusy)
    ));
}

async fn assert_listener_unavailable(address: std::net::SocketAddr) {
    match timeout(
        Duration::from_millis(500),
        tokio::net::TcpStream::connect(address),
    )
    .await
    {
        Ok(Ok(stream)) => {
            panic!("HTTPS listener was reachable before product startup committed: {stream:?}")
        }
        Ok(Err(_)) | Err(_) => {}
    }
}

#[tokio::test]
async fn production_factory_opens_listener_only_after_the_bundle_is_complete() {
    use crate::node::session_services::{FactoryStage, FactoryTestHooks};

    let area = TestArea::new();
    let address = free_address();
    let before_services = Arc::new(crate::test_support::AsyncBarrier::default());
    let mut hooks = FactoryTestHooks::default();
    hooks.before_services = Some(before_services.clone());
    let owner = NodeProcessOwner::begin_for_factory_test(
        &Handle::current(),
        area.config(address),
        Arc::new(FakeProtector::default()),
        hooks,
    );
    let ready = owner.ready();

    timeout(Duration::from_secs(5), before_services.wait_until_entered())
        .await
        .expect("production startup reached BeforeServices");
    assert_listener_unavailable(address).await;
    let gated = owner.factory_observation_for_test();
    assert_eq!(gated.stage, FactoryStage::BeforeServices);
    assert!(!gated.services_installed && !gated.ready);
    assert!(
        lock(&owner.host.operation.resources)
            .session_services
            .is_none()
    );

    before_services.release();
    let node = ready.await.expect("complete bundle publishes");
    let committed = owner.factory_observation_for_test();
    assert_eq!(committed.operation, gated.operation);
    assert_eq!(committed.stage, FactoryStage::Ready);
    assert!(committed.services_installed && committed.ready);
    assert!(Arc::ptr_eq(
        node.session_services.as_ref().expect("production bundle"),
        lock(&owner.host.operation.resources)
            .session_services
            .as_ref()
            .expect("same lifecycle-owned bundle")
    ));

    node.shutdown(None)
        .await
        .expect("complete product bundle shuts down");
    timeout(Duration::from_secs(5), owner.join_actual())
        .await
        .expect("lifecycle join deadline")
        .expect("lifecycle join");
    assert!(DataDirectory::acquire(&area.data_dir()).is_ok());
}

#[tokio::test]
async fn production_factory_service_rejection_opens_no_listener_and_joins_workers() {
    use crate::node::session_services::FactoryTestHooks;

    let area = TestArea::new();
    let address = free_address();
    let mut hooks = FactoryTestHooks::default();
    hooks.reject_services = true;
    let owner = NodeProcessOwner::begin_for_factory_test(
        &Handle::current(),
        area.config(address),
        Arc::new(FakeProtector::default()),
        hooks,
    );

    let error = match owner.ready().await {
        Ok(node) => {
            drop(node);
            panic!("injected service failure unexpectedly became ready");
        }
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("injected session service installation failure"),
        "{error}"
    );
    assert_listener_unavailable(address).await;
    let storage = lock(&owner.host.operation.resources)
        .storage
        .clone()
        .expect("lifecycle retains the original Storage");
    let preparation = storage
        .prompt_preparation_receipt()
        .expect("preparation receipt installed by the original bundle");
    let report = timeout(Duration::from_secs(10), preparation.join())
        .await
        .expect("preparation join deadline");
    assert!(
        report.observation().all_joined(),
        "preparation workers did not actually join: {report:?}"
    );
    timeout(Duration::from_secs(10), owner.join_actual())
        .await
        .expect("lifecycle join deadline")
        .expect("lifecycle join");
    assert!(
        lock(&owner.host.operation.resources)
            .session_services
            .is_none()
    );
    assert!(DataDirectory::acquire(&area.data_dir()).is_ok());
}

#[tokio::test]
async fn registration_precedes_open_and_cancelled_ready_stops_original_scope() {
    let area = TestArea::new();
    let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
    let protector: Arc<dyn SecretProtector> = Arc::new(FakeProtector::default());
    let owner = NodeProcessOwner::begin_inner(
        &Handle::current(),
        area.config(free_address()),
        protector.clone(),
        DevHooks {
            control_only: true,
            before_open: Some(barrier.clone()),
            ..DevHooks::default()
        },
    );
    let operation = owner.host.operation.clone();
    let host = Arc::downgrade(&owner.host);
    let waiter = tokio::spawn(async move { owner.ready().await });
    timeout(Duration::from_secs(2), barrier.wait_until_entered())
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&operation.protector, &protector));
    assert!(lock(&operation.resources).storage.is_none());
    locked(&operation, &area);
    waiter.abort();
    assert!(matches!(waiter.await, Err(error) if error.is_cancelled()));
    assert!(operation.stop.borrow().is_some());
    barrier.release();
    actual(&operation, Actual::Joined).await;
    assert!(
        lock(&operation.resources).storage.is_none(),
        "stop before the original open gate creates no successful Storage"
    );
    assert!(
        host.upgrade().is_some(),
        "the original handle owner remains installed until actual observation"
    );
    assert!(
        Arc::strong_count(&operation) >= 2,
        "the live original driver still holds this ledger"
    );
    locked(&operation, &area);
}
#[tokio::test]
async fn consumed_waiter_cancellation_preserves_original_storage_and_budget() {
    let area = TestArea::new();
    let runtime = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let host = runtime.lifecycle.clone();
    let original_storage = runtime.storage.as_ref().unwrap().clone();
    let slots = lock(&host.operation.tasks).clone().unwrap();
    let barrier = original_storage.pause_next_close();
    let waiter = tokio::spawn(runtime.shutdown_with_deadline(None, Duration::from_millis(250)));
    timeout(Duration::from_secs(2), barrier.wait_until_entered())
        .await
        .unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    let error = timeout(Duration::from_secs(2), host.budget())
        .await
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("closing storage"), "{error}");
    assert!(Arc::ptr_eq(
        lock(&host.operation.resources).storage.as_ref().unwrap(),
        &original_storage
    ));
    assert!(Arc::ptr_eq(
        lock(&host.operation.tasks).as_ref().unwrap(),
        &slots
    ));
    locked(&host.operation, &area);
    let first = host.operation.budget.borrow().clone();
    barrier.release();
    actual(&host.operation, Actual::Joined).await;
    assert_eq!(*host.operation.budget.borrow(), first);
    host.join_actual().await.unwrap();
    assert!(host.task.lock().await.is_none());
    assert!(DataDirectory::acquire(&area.data_dir()).is_ok());
}
#[tokio::test]
async fn two_budget_waiters_share_one_stop_and_cancelling_one_does_not_restart_it() {
    let area = TestArea::new();
    let owner = owner(&area);
    let runtime = owner.ready().await.unwrap();
    let host = owner.host.clone();
    let storage = runtime.storage.as_ref().unwrap().clone();
    let barrier = storage.pause_next_close();
    host.operation
        .request_stop(None, Duration::from_millis(250));
    let first_start = host.operation.stop.borrow().as_ref().unwrap().started;
    let first = {
        let host = host.clone();
        tokio::spawn(async move { host.budget().await })
    };
    let second = {
        let host = host.clone();
        tokio::spawn(async move { host.budget().await })
    };
    timeout(Duration::from_secs(2), barrier.wait_until_entered())
        .await
        .unwrap();
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert!(second.await.unwrap().is_err());
    host.operation.request_stop(None, SHUTDOWN_TIMEOUT);
    assert_eq!(
        host.operation.stop.borrow().as_ref().unwrap().started,
        first_start
    );
    assert_eq!(
        host.operation.stop.borrow().as_ref().unwrap().timeout,
        Duration::from_millis(250)
    );
    barrier.release();
    actual(&host.operation, Actual::Joined).await;
    drop(runtime);
}
#[tokio::test]
async fn last_data_drop_on_thread_without_runtime_only_seals_original_owner() {
    let area = TestArea::new();
    let owner = owner(&area);
    let runtime = owner.ready().await.unwrap();
    let storage = runtime.storage.as_ref().unwrap().clone();
    std::thread::spawn(move || {
        assert!(Handle::try_current().is_err());
        drop(runtime);
    })
    .join()
    .unwrap();
    actual(&owner.host.operation, Actual::Joined).await;
    assert_eq!(owner.host.operation.data_handles.load(Ordering::Acquire), 0);
    assert!(Arc::ptr_eq(
        lock(&owner.host.operation.resources)
            .storage
            .as_ref()
            .unwrap(),
        &storage
    ));
    owner.join_actual().await.unwrap();
    assert!(owner.host.task.lock().await.is_none());
    assert!(DataDirectory::acquire(&area.data_dir()).is_ok());
}
#[tokio::test]
async fn last_host_drop_preserves_original_handle_until_actual_join() {
    let area = TestArea::new();
    let mut owner = owner(&area);
    let runtime = owner.ready().await.unwrap();
    let operation = Arc::downgrade(&owner.host.operation);
    let host = Arc::downgrade(&owner.host);
    owner.disarm();
    drop(runtime);
    drop(owner);
    let operation = operation
        .upgrade()
        .expect("original installed driver owner still owns ledger");
    actual(&operation, Actual::Joined).await;
    let installed = host
        .upgrade()
        .expect("original top-level handle remains installed");
    locked(&operation, &area);
    installed.join_actual().await.unwrap();
    drop(installed);
    assert!(host.upgrade().is_none());
    assert!(DataDirectory::acquire(&area.data_dir()).is_ok());
}
#[tokio::test]
async fn real_tls_failure_rolls_back_same_storage_and_joins_original_workers() {
    let area = TestArea::new();
    let mut config = area.config(free_address());
    config.tls_cert = area.data_dir().join("missing-certificate.pem");
    let owner = NodeProcessOwner::begin_control_only_for_test(
        &Handle::current(),
        config,
        Arc::new(FakeProtector::default()),
    );
    assert!(matches!(
        owner.ready().await,
        Err(AppError::Io { .. } | AppError::Tls(_))
    ));
    actual(&owner.host.operation, Actual::Joined).await;
    assert!(lock(&owner.host.operation.resources).storage.is_some());
    locked(&owner.host.operation, &area);
    owner.join_actual().await.unwrap();
    assert!(owner.host.task.lock().await.is_none());
    assert!(DataDirectory::acquire(&area.data_dir()).is_ok());
}
#[tokio::test]
async fn outer_driver_survives_unwind_and_retains_original_registered_storage() {
    let area = TestArea::new();
    let owner = NodeProcessOwner::begin_inner(
        &Handle::current(),
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
        DevHooks {
            control_only: true,
            panic_after_storage: true,
            ..DevHooks::default()
        },
    );
    assert!(owner.ready().await.is_err());
    actual(&owner.host.operation, Actual::DriverUnconfirmed).await;
    let error = owner.host.budget().await.unwrap_err();
    assert!(error.to_string().contains("unwound"));
    assert!(lock(&owner.host.operation.resources).storage.is_some());
    assert!(
        !owner.host.task.lock().await.as_ref().unwrap().is_finished(),
        "outer catch does not return and drop guard"
    );
    locked(&owner.host.operation, &area);
}
#[tokio::test]
async fn failure_before_any_database_acquisition_can_finish_without_claiming_sqlite_closed() {
    let area = TestArea::new();
    std::fs::write(area.data_dir(), b"not a directory").unwrap();
    let owner = owner(&area);
    assert!(owner.ready().await.is_err());
    timeout(Duration::from_secs(2), owner.join_actual())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        *owner.host.operation.actual.borrow(),
        Actual::NoDatabaseAcquired
    );
    assert!(!lock(&owner.host.operation.resources).database_started);
    assert!(lock(&owner.host.operation.resources).storage.is_none());
}
#[tokio::test]
async fn joined_transport_and_actual_sqlite_close_publish_completed() {
    let area = TestArea::new();
    let owner = owner(&area);
    let runtime = owner.ready().await.unwrap();
    owner
        .host
        .operation
        .request_stop(None, Duration::from_secs(2));
    assert!(owner.host.budget().await.is_ok());
    actual(&owner.host.operation, Actual::Joined).await;
    let slots = lock(&owner.host.operation.tasks).clone().unwrap();
    let tasks = slots.lock().await;
    assert!(tasks.network.as_ref().unwrap().is_finished());
    assert!(tasks.ipc.as_ref().unwrap().is_finished());
    drop(tasks);
    assert!(matches!(
        *owner.host.operation.budget.borrow(),
        ShutdownResult::Completed
    ));
    assert!(matches!(
        *runtime.control.shutdown_receiver().borrow(),
        ShutdownResult::Completed
    ));
    locked(&owner.host.operation, &area);
}

#[tokio::test]
async fn cancelled_ready_waiter_seals_live_owner_and_publishes_budget_during_acquisition() {
    let area = TestArea::new();
    let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
    let owner = NodeProcessOwner::begin_inner(
        &Handle::current(),
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
        DevHooks {
            control_only: true,
            before_open: Some(barrier.clone()),
            ..DevHooks::default()
        },
    );
    let host = owner.host.clone();
    let waiter = {
        let host = host.clone();
        tokio::spawn(async move { host.ready().await })
    };
    timeout(Duration::from_secs(2), barrier.wait_until_entered())
        .await
        .unwrap();
    waiter.abort();
    assert!(matches!(waiter.await, Err(error) if error.is_cancelled()));
    host.operation
        .request_stop(None, Duration::from_millis(250));
    let started = host.operation.stop.borrow().as_ref().unwrap().started;
    assert!(
        timeout(Duration::from_secs(2), host.budget())
            .await
            .unwrap()
            .is_err()
    );
    assert!(
        lock(&host.operation.resources).storage.is_none(),
        "original open is still paused"
    );
    locked(&host.operation, &area);
    assert!(!host.task.lock().await.as_ref().unwrap().is_finished());
    let first = host.operation.budget.borrow().clone();
    barrier.release();
    actual(&host.operation, Actual::Joined).await;
    assert_eq!(
        host.operation.stop.borrow().as_ref().unwrap().started,
        started
    );
    assert_eq!(*host.operation.budget.borrow(), first);
    assert!(lock(&host.operation.resources).storage.is_none());
    locked(&host.operation, &area);
    host.join_actual().await.unwrap();
}

#[tokio::test]
async fn ready_projection_uses_the_original_ledger_resources() {
    let area = TestArea::new();
    let protector: Arc<dyn SecretProtector> = Arc::new(FakeProtector::default());
    let owner = NodeProcessOwner::begin_control_only_for_test(
        &Handle::current(),
        area.config(free_address()),
        protector.clone(),
    );
    let runtime = owner.ready().await.unwrap();
    {
        let roots = lock(&owner.host.operation.resources);
        assert!(Arc::ptr_eq(&owner.host.operation.protector, &protector));
        assert!(Arc::ptr_eq(
            runtime.storage.as_ref().unwrap(),
            roots.storage.as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(
            runtime.identity.as_ref().unwrap(),
            roots.identity.as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(
            runtime.pairing.as_ref().unwrap(),
            roots.pairing.as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(
            runtime.management.as_ref().unwrap(),
            roots.management.as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(
            runtime.application_policy.as_ref().unwrap(),
            roots.policy.as_ref().unwrap()
        ));
        assert_eq!(
            runtime.data_dir.as_ref().unwrap().database_path(),
            roots.directory.as_ref().unwrap().database_path()
        );
    }
    owner.host.operation.request_stop(None, SHUTDOWN_TIMEOUT);
    assert!(owner.host.budget().await.is_ok());
    actual(&owner.host.operation, Actual::Joined).await;
    locked(&owner.host.operation, &area);
    drop(runtime);
}
#[tokio::test]
async fn original_fifteen_second_budget_publishes_while_real_close_stays_blocked() {
    let area = TestArea::new();
    let owner = owner(&area);
    let runtime = owner.ready().await.unwrap();
    let storage = runtime.storage.as_ref().unwrap().clone();
    let barrier = storage.pause_next_close();
    let started = Instant::now();
    owner.host.operation.request_stop(None, SHUTDOWN_TIMEOUT);
    timeout(Duration::from_secs(2), barrier.wait_until_entered())
        .await
        .unwrap();
    assert!(
        timeout(Duration::from_secs(16), owner.host.budget())
            .await
            .unwrap()
            .is_err()
    );
    assert!(started.elapsed() >= SHUTDOWN_TIMEOUT.saturating_sub(SHUTDOWN_RESPONSE_TIMEOUT));
    assert_eq!(
        owner.host.operation.stop.borrow().as_ref().unwrap().timeout,
        Duration::from_secs(15)
    );
    assert!(!owner.host.task.lock().await.as_ref().unwrap().is_finished());
    assert!(Arc::ptr_eq(
        lock(&owner.host.operation.resources)
            .storage
            .as_ref()
            .unwrap(),
        &storage
    ));
    locked(&owner.host.operation, &area);
    let first = owner.host.operation.budget.borrow().clone();
    barrier.release();
    actual(&owner.host.operation, Actual::Joined).await;
    assert_eq!(*owner.host.operation.budget.borrow(), first);
    drop(runtime);
}

#[tokio::test]
async fn network_accept_error_keeps_original_child_until_real_drain() {
    network_error_drain(true).await;
}
#[tokio::test]
async fn network_child_panic_joins_the_original_transport_task() {
    network_error_drain(false).await;
}
async fn network_error_drain(accept_error: bool) {
    let area = TestArea::new();
    let config = area.config(free_address());
    let tls =
        TlsAcceptor::from(tls::load_server_config(&config.tls_cert, &config.tls_key).unwrap());
    let listener = TcpListener::bind(config.listen).await.unwrap();
    let child = Arc::new(crate::test_support::AsyncBarrier::default());
    let hooks = Arc::new(NetworkDevelopmentHooks {
        child: child.clone(),
        accept_error_after_first: accept_error,
        child_panics: !accept_error,
    });
    let (_stop, stop) = watch::channel(false);
    let task = tokio::spawn(run_network_inner(
        listener,
        tls,
        stop,
        Router::new(),
        Some(hooks),
    ));
    let client = TcpStream::connect(config.listen).await.unwrap();
    timeout(Duration::from_secs(2), child.wait_until_entered())
        .await
        .unwrap();
    assert!(
        !task.is_finished(),
        "the original child is retained during drain"
    );
    drop(client);
    child.release();
    let result = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(
        result.contains(if accept_error {
            "development injected HTTPS accept failure"
        } else {
            "HTTPS connection task failed"
        }),
        "{result}"
    );
    assert!(
        TcpListener::bind(config.listen).await.is_ok(),
        "original listener was dropped"
    );
}

#[tokio::test]
async fn ipc_accept_error_waits_for_original_pipe_handler_drain() {
    ipc_error_drain(true).await;
}
#[tokio::test]
async fn ipc_two_second_deadline_aborts_and_joins_the_original_pipe_handler() {
    ipc_error_drain(false).await;
}
async fn ipc_error_drain(accept_error: bool) {
    let area = TestArea::new();
    let child = Arc::new(crate::test_support::AsyncBarrier::default());
    let hooks = Arc::new(ipc::IpcDevelopmentHooks {
        child: child.clone(),
        accept_error_after_first: accept_error,
    });
    let owner = NodeProcessOwner::begin_inner(
        &Handle::current(),
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
        DevHooks {
            control_only: true,
            ipc: Some(hooks),
            ..DevHooks::default()
        },
    );
    let runtime = owner.ready().await.unwrap();
    let client = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(&runtime.ipc_address().pipe_name)
        .unwrap();
    timeout(Duration::from_secs(2), child.wait_until_entered())
        .await
        .unwrap();
    {
        let slots = lock(&owner.host.operation.tasks).clone().unwrap();
        let tasks = slots.lock().await;
        assert!(!tasks.ipc.as_ref().unwrap().is_finished());
        drop(tasks);
    }
    let started = Instant::now();
    owner
        .host
        .operation
        .request_stop(None, Duration::from_millis(250));
    assert!(owner.host.budget().await.is_err());
    let first = owner.host.operation.budget.borrow().clone();
    if accept_error {
        drop(client);
        child.release();
    }
    actual(&owner.host.operation, Actual::Joined).await;
    if !accept_error {
        assert!(started.elapsed() >= Duration::from_secs(2));
        child.release();
    }
    let errors = lock(&owner.host.operation.resources).failures.join("; ");
    assert!(
        errors.contains(if accept_error {
            "development injected IPC accept failure"
        } else {
            "owner control handlers did not stop in time"
        }),
        "{errors}"
    );
    assert_eq!(*owner.host.operation.budget.borrow(), first);
    let slots = lock(&owner.host.operation.tasks).clone().unwrap();
    assert!(slots.lock().await.ipc.as_ref().unwrap().is_finished());
    locked(&owner.host.operation, &area);
    drop(runtime);
}

#[cfg(feature = "native-dispatch-dev-fixtures")]
#[tokio::test]
async fn original_native_installation_wins_before_stop_and_same_supervisor_is_joined() {
    native_install_stop(true).await;
}
#[cfg(feature = "native-dispatch-dev-fixtures")]
#[tokio::test]
async fn original_stop_wins_before_native_installation_and_creates_no_claim() {
    native_install_stop(false).await;
}
#[cfg(feature = "native-dispatch-dev-fixtures")]
async fn native_install_stop(install_wins: bool) {
    use crate::{
        adapters::acp::native_dispatch::AcpNativeAdapter,
        application::{
            native_dispatch::dev_tests::{Configuration, Retain, Validator, options},
            request_registry::SystemNodeClock,
        },
        domain::native_dispatch::DispatchError,
        platform::native_source::WindowsSourceLauncher,
    };
    use std::future::Future as _;
    let area = TestArea::new();
    let owner = owner(&area);
    let runtime = owner.ready().await.unwrap();
    let storage = runtime.storage.as_ref().unwrap();
    let configuration = Arc::new(storage.native_configuration_provider(Arc::new(Configuration)));
    let authority = runtime.request_authority().unwrap();
    let start = storage.start_native_service(
        authority,
        Validator,
        SystemNodeClock,
        Arc::new(AcpNativeAdapter::default()),
        Arc::new(WindowsSourceLauncher),
        configuration,
        Arc::new(Retain),
        options(),
    );
    tokio::pin!(start);
    if install_wins {
        let polled =
            std::future::poll_fn(|cx| std::task::Poll::Ready(start.as_mut().poll(cx))).await;
        assert!(
            polled.is_pending(),
            "first actual startup await follows original supervisor installation"
        );
        let installed = storage.original_runtime_registration_for_test();
        assert!(installed.0 && installed.1.is_some());
        owner
            .host
            .operation
            .request_stop(None, Duration::from_secs(5));
        assert_eq!(storage.original_runtime_registration_for_test(), installed);
        let (queue, native) = start.await.unwrap();
        assert!(owner.host.budget().await.is_ok());
        actual(&owner.host.operation, Actual::Joined).await;
        let report = native
            .shutdown(Instant::now() + Duration::from_secs(2))
            .await;
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        assert_eq!(
            storage.original_runtime_registration_for_test(),
            (false, None)
        );
        drop((queue, native));
    } else {
        owner
            .host
            .operation
            .request_stop(None, Duration::from_secs(5));
        let before_start = storage.original_runtime_registration_for_test();
        let rejected = matches!(start.await, Err(DispatchError::NodeStopping));
        let after_start = storage.original_runtime_registration_for_test();
        let budget = owner.host.budget().await;
        actual(&owner.host.operation, Actual::Joined).await;
        let completed = matches!(
            *owner.host.operation.budget.borrow(),
            ShutdownResult::Completed
        );
        let directory_retained = lock(&owner.host.operation.resources).directory.is_some();
        let still_locked = matches!(
            DataDirectory::acquire(&area.data_dir()),
            Err(AppError::DataDirectoryBusy)
        );
        let joined = owner.join_actual().await;
        let released = DataDirectory::acquire(&area.data_dir()).is_ok();
        assert_eq!(before_start, (false, None));
        assert!(rejected);
        assert_eq!(after_start, (false, None));
        assert!(budget.is_ok(), "normal rejected startup: {budget:?}");
        assert!(completed);
        assert!(directory_retained && still_locked);
        assert!(joined.is_ok(), "original physical join: {joined:?}");
        assert!(released);
        return;
    }
    locked(&owner.host.operation, &area);
}

#[cfg(feature = "native-dispatch-dev-fixtures")]
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep the real Native metadata and original CPU cancellation timeline visible in one development case"
)]
async fn original_native_metadata_close_advances_while_real_body_cpu_remains_owned() {
    use crate::{
        adapters::acp::native_dispatch::AcpNativeAdapter,
        application::{
            native_dispatch::dev_tests::{Configuration, options},
            request_registry::SystemNodeClock,
        },
        node::prompt_preparation::{
            attach_with_hooks,
            test_hooks::{Hooks, Point},
        },
        platform::native_source::WindowsSourceLauncher,
    };
    struct CpuPause {
        entered: tokio::sync::Notify,
        released: Mutex<bool>,
        cv: std::sync::Condvar,
    }
    impl CpuPause {
        fn release(&self) {
            *lock(&self.released) = true;
            self.cv.notify_all();
        }
    }
    struct ReleaseCpu(Arc<CpuPause>);
    impl Drop for ReleaseCpu {
        fn drop(&mut self) {
            self.0.release();
        }
    }
    let area = TestArea::new();
    let owner = owner(&area);
    let runtime = owner.ready().await.unwrap();
    let storage = runtime.storage.as_ref().unwrap();
    let adapter = Arc::new(AcpNativeAdapter::default());
    let configuration = Arc::new(storage.native_configuration_provider(Arc::new(Configuration)));
    let (queue, native) = storage
        .start_native_service(
            runtime.request_authority().unwrap(),
            adapter.clone(),
            SystemNodeClock,
            adapter.clone(),
            Arc::new(WindowsSourceLauncher),
            configuration,
            adapter.clone(),
            options(),
        )
        .await
        .unwrap();
    let pause = Arc::new(CpuPause {
        entered: tokio::sync::Notify::new(),
        released: Mutex::new(false),
        cv: std::sync::Condvar::new(),
    });
    let _release = ReleaseCpu(pause.clone());
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observer_pause = pause.clone();
    let prepared = attach_with_hooks(
        storage,
        queue.clone(),
        Hooks {
            observer: Some(Arc::new(move |point| {
                if point == Point::RawParsedBeforeBlockChecks
                    && !stopped.swap(true, Ordering::AcqRel)
                {
                    observer_pause.entered.notify_one();
                    let mut released = lock(&observer_pause.released);
                    while !*released {
                        released = observer_pause.cv.wait(released).unwrap();
                    }
                    drop(released);
                }
            })),
            queue: None,
        },
    )
    .unwrap();
    let body = prepared
        .begin(br#"[{"type":"text","text":"lifecycle cpu original"}]"#.to_vec())
        .unwrap();
    timeout(Duration::from_secs(5), pause.entered.notified())
        .await
        .unwrap();
    let receipt = storage.prompt_preparation_receipt().unwrap();
    let original_identity = body.observe().identity().clone();
    ipc::status(runtime.ipc_address()).await.unwrap();
    storage.list_revision().await.unwrap();
    owner
        .host
        .operation
        .request_stop(None, Duration::from_millis(250));
    assert!(owner.host.budget().await.is_err());
    let report = native
        .shutdown(Instant::now() + Duration::from_secs(2))
        .await;
    assert!(report.joined && report.metadata_confirmed, "{report:?}");
    assert!(
        !receipt.observe().all_joined(),
        "body CPU is still the original blocked task"
    );
    assert!(original_identity.same(body.observe().identity()));
    storage.list_revision().await.unwrap();
    locked(&owner.host.operation, &area);
    pause.release();
    drop((body, prepared));
    let closed = timeout(Duration::from_secs(5), receipt.join())
        .await
        .unwrap();
    assert!(closed.observation().all_joined());
    actual(&owner.host.operation, Actual::Joined).await;
    locked(&owner.host.operation, &area);
    drop((queue, native));
}

#[tokio::test]
async fn sqlite_weak_probe_observes_original_operation_and_joins_after_last_owner_drop() {
    use crate::session_storage::sqlite_lifecycle::{TestControl, TestPoint};
    let area = TestArea::new();
    let control = TestControl::default();
    let barrier = control.pause_next(TestPoint::ParentInstalledBeforeDriverGate);
    let owner = NodeProcessOwner::begin_for_sqlite_test(
        &Handle::current(),
        area.config(free_address()),
        Arc::new(FakeProtector::default()),
        control,
    );
    let probe = owner.probe_for_sqlite_test();
    timeout(Duration::from_secs(5), barrier.arrived())
        .await
        .unwrap();
    let sqlite = owner.sqlite_state_for_test().unwrap();
    let first = probe.snapshot().unwrap().unwrap();
    let second = probe.snapshot().unwrap().unwrap();
    assert_eq!(first.original_operation, second.original_operation);
    assert_eq!(first.protector, second.protector);
    assert_eq!(first.driver_installed, Some(true));
    assert_eq!(first.budget, ShutdownResult::Pending);
    assert_eq!(first.actual, Actual::Acquiring);
    assert!(first.directory_retained);
    assert!(std::sync::Weak::ptr_eq(
        first.sqlite.as_ref().unwrap(),
        &sqlite
    ));
    drop(owner);
    barrier.release();
    timeout(Duration::from_secs(5), probe.observe_actual())
        .await
        .unwrap()
        .unwrap();
    assert!(probe.snapshot().unwrap().is_none());
    assert!(sqlite.upgrade().is_none());
    assert!(DataDirectory::acquire(&area.data_dir()).is_ok());
}
