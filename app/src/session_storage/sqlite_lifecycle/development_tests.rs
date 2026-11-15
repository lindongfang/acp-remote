use super::*;
use sqlx::sqlite::WorkerHandleState;
use std::os::windows::fs::OpenOptionsExt;
static DEV_AREA_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn roots(label: &str) -> StorageOpenRoots {
    let base = std::env::var_os("CLDB_DEV_DATA")
        .filter(|v| !v.is_empty())
        .map_or_else(
            || {
                std::env::temp_dir().join(format!(
                    "acpr-cldb-dev-{}-{}",
                    std::process::id(),
                    DEV_AREA_SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ))
            },
            PathBuf::from,
        );
    let path = base.join(label);
    assert!(!path.exists());
    std::fs::create_dir_all(&path).unwrap();
    let directory = Arc::new(crate::data_dir::DataDirectory::acquire(&path).unwrap());
    StorageOpenRoots::directory(directory)
}
fn exclusive(path: &Path) {
    let _file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
        .unwrap();
}
#[tokio::test]
async fn dev_prepare_is_synchronous_no_driver_or_worker() {
    let owner = StorageOpenOwner::prepare(&Handle::current(), roots("prepare-only")).unwrap();
    let snapshot = owner.state.test_snapshot();
    assert!(!snapshot.driver.parent_installed);
    assert!(!snapshot.driver.ready);
    assert!(!snapshot.sql.stop && !snapshot.sql.final_fence);
    assert!(snapshot.sql.users_done);
    assert_eq!(snapshot.retained_root_count, 1);
    assert!(snapshot.pool.is_none());
    assert_eq!(
        snapshot.open_operation,
        owner.state.test_snapshot().open_operation
    );
    assert_eq!(snapshot.driver.handle_installed, Some(false));
    assert!(snapshot.scope.unwrap().workers.is_empty());
    let host = StorageLifecycleHost::install(owner.clone());
    owner.request_stop();
    assert!(host.observe_actual().await);
    assert!(!owner.state.test_snapshot().roots_retained);
}
#[tokio::test]
async fn dev_ready_cancel_only_observes_original_installed_driver() {
    let control = TestControl::default();
    let barrier = control.pause_next(TestPoint::ParentInstalledBeforeDriverGate);
    let owner =
        StorageOpenOwner::prepare_for_test(&Handle::current(), roots("ready-cancel"), control)
            .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let ticket = Arc::new(owner.start().unwrap());
    let waiter = {
        let ticket = ticket.clone();
        tokio::spawn(async move { ticket.ready().await })
    };
    barrier.arrived().await;
    let before = owner.state.test_snapshot();
    assert!(before.driver.parent_installed);
    assert_eq!(before.driver.handle_installed, Some(true));
    assert!(!before.driver.gate_open);
    assert!(before.scope.unwrap().workers.is_empty());
    waiter.abort();
    match waiter.await {
        Err(error) => assert!(error.is_cancelled()),
        Ok(_) => panic!("ready observer unexpectedly completed"),
    }
    assert!(owner.state.test_snapshot().roots_retained);
    barrier.release();
    assert!(host.observe_actual().await);
    assert!(!owner.state.test_snapshot().roots_retained);
}
#[tokio::test]
async fn dev_original_std_join_does_not_claim_app_task_join() {
    let control = TestControl::default();
    let barrier = control.pause_next(TestPoint::AppJoinResultBeforeHandleObservation);
    let owner =
        StorageOpenOwner::prepare_for_test(&Handle::current(), roots("std-before-app"), control)
            .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let storage = owner.start().unwrap().ready().await.unwrap();
    let path = lock(&owner.state.roots).as_ref().unwrap().path.clone();
    owner.request_stop();
    barrier.arrived().await;
    let snapshot = owner.state.test_snapshot();
    assert!(
        snapshot
            .scope
            .unwrap()
            .workers
            .iter()
            .any(|w| w.handle == WorkerHandleState::Joined)
    );
    assert!(
        snapshot
            .joins
            .iter()
            .any(|j| j.state != AppJoinState::Joined && j.handle_installed == Some(true))
    );
    assert!(snapshot.roots_retained);
    assert!(snapshot.driver.ready && snapshot.driver.gate_open);
    assert!(snapshot.sql.stop && snapshot.sql.final_fence && snapshot.sql.users_done);
    assert!(snapshot.pool.is_some());
    let second = owner.state.test_snapshot();
    for join in &snapshot.joins {
        let original = second
            .joins
            .iter()
            .find(|other| other.worker == join.worker)
            .unwrap();
        assert_eq!(join.operation, original.operation);
        assert!(!join.task_join_error);
    }
    let observer = {
        let owner = owner.clone();
        tokio::spawn(async move { owner.observe_actual().await })
    };
    tokio::task::yield_now().await;
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    barrier.release();
    assert!(host.observe_actual().await);
    exclusive(&path);
    drop(storage);
}
#[test]
fn dev_queued_app_join_keeps_original_std_handle_in_vendor() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let (release, held) = std::sync::mpsc::channel();
        let (entered, entry) = tokio::sync::oneshot::channel();
        let blocker = Handle::current().spawn_blocking(move || {
            let _ = entered.send(());
            held.recv().unwrap();
        });
        entry.await.unwrap();
        let control = TestControl::default();
        let gate = control.pause_next(TestPoint::AppJoinHandleInstalledBeforeGate);
        let owner =
            StorageOpenOwner::prepare_for_test(&Handle::current(), roots("queued-join"), control)
                .unwrap();
        let host = StorageLifecycleHost::install(owner.clone());
        let storage = owner.start().unwrap().ready().await.unwrap();
        owner.request_stop();
        gate.arrived().await;
        let snapshot = owner.state.test_snapshot();
        assert!(
            snapshot
                .joins
                .iter()
                .any(|j| j.state == AppJoinState::Queued && j.handle_installed == Some(true))
        );
        assert!(
            snapshot
                .scope
                .unwrap()
                .workers
                .iter()
                .any(|w| w.handle == WorkerHandleState::VendorOwned)
        );
        gate.release();
        tokio::task::yield_now().await;
        release.send(()).unwrap();
        blocker.await.unwrap();
        assert!(host.observe_actual().await);
        drop(storage);
    });
}

#[tokio::test]
async fn dev_required_sql_report_loss_keeps_original_roots_after_physical_join() {
    let owner =
        StorageOpenOwner::prepare(&Handle::current(), roots("required-report-loss")).unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let storage = owner.start().unwrap().ready().await.unwrap();
    let directory = lock(&owner.state.roots)
        .as_ref()
        .unwrap()
        .path
        .parent()
        .unwrap()
        .to_owned();
    let registry = storage.request_registry();
    let connection = registry.detached_owned_connection().await.unwrap();
    registry
        .connection_closer_for_test()
        .fail_next_result_after_ack()
        .unwrap();
    drop(connection);
    owner.request_stop();
    assert!(!host.observe_actual().await);
    let snapshot = owner.state.test_snapshot();
    assert_eq!(snapshot.actual, Actual::Unconfirmed);
    assert!(snapshot.roots_retained);
    assert!(snapshot.scope.unwrap().all_threads_terminated());
    assert!(
        snapshot
            .joins
            .iter()
            .all(|j| j.state == AppJoinState::Joined)
    );
    assert_eq!(
        storage.required_close_status_in_scope(),
        super::super::ConnectionCloseStatus::Unconfirmed
    );
    let weak = Arc::downgrade(&owner.state);
    drop(registry);
    drop(storage);
    drop(owner);
    drop(host);
    assert!(weak.upgrade().unwrap().test_snapshot().roots_retained);
    assert!(crate::data_dir::DataDirectory::acquire(&directory).is_err());
}
#[tokio::test]
async fn dev_raw_fixture_scope_retains_original_owner_and_joins() {
    use sqlx::Connection;
    let original = roots("raw-original-owner");
    let path = original.path.clone();
    let original = Arc::new(original);
    let weak = Arc::downgrade(&original);
    let owner = StorageOpenOwner::prepare(
        &Handle::current(),
        StorageOpenRoots::retained(&path, original.clone()),
    )
    .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let ticket = owner.start_worker_scope().unwrap();
    ticket.preflight_ready().await.unwrap();
    assert!(
        owner
            .state
            .test_snapshot()
            .scope
            .unwrap()
            .workers
            .is_empty()
    );
    drop(original);
    assert!(weak.upgrade().is_some());
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .worker_registry(owner.state.registry().clone());
    let mut connection = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE actual_fixture(value INTEGER)")
        .execute(&mut connection)
        .await
        .unwrap();
    connection.close().await.unwrap();
    owner.request_stop();
    assert!(host.observe_actual().await);
    assert!(weak.upgrade().is_none());
    assert!(!owner.state.test_snapshot().roots_retained);
    exclusive(&path);
}

#[tokio::test]
async fn dev_original_sql_receipt_keeps_scope_worker_and_operation_axes() {
    use super::super::registry_connection_owner::{CloseOutcome, OriginalCleanupObservation};
    let owner = StorageOpenOwner::prepare(&Handle::current(), roots("sql-receipt")).unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let storage = owner.start().unwrap().ready().await.unwrap();
    let registry = storage.request_registry();
    let connection = registry.detached_owned_connection().await.unwrap();
    let waiter = connection.begin_owned_close().unwrap();
    let receipt = waiter.receipt();
    let before = receipt.snapshot_for_test();
    assert_eq!(
        before.scope,
        Some(owner.state.test_snapshot().scope.unwrap().scope)
    );
    assert_eq!(waiter.wait_report().await, CloseOutcome::Confirmed);
    assert_eq!(
        receipt
            .confirm_actual_cleanup(tokio::time::Instant::now() + std::time::Duration::from_secs(5))
            .await,
        OriginalCleanupObservation::ConfirmedJoined
    );
    let after = receipt.snapshot_for_test();
    assert_eq!(before.owner, after.owner);
    assert_eq!(before.operation, after.operation);
    assert_eq!(before.scope, after.scope);
    assert_eq!(before.worker.unwrap().id, after.worker.unwrap().id);
    assert_eq!(
        (after.sql_actual, after.sql_task_join, after.sql_report),
        (1, 1, 1)
    );
    assert_eq!(after.app_join, Some(true));
    owner.request_stop();
    assert!(host.observe_actual().await);
}

#[tokio::test]
async fn dev_app_result_unavailable_keeps_original_roots_after_std_join() {
    let control = TestControl::default();
    let gate = control.pause_next(TestPoint::AppJoinResultBeforeHandleObservation);
    let owner = StorageOpenOwner::prepare_for_test(
        &Handle::current(),
        roots("result-unavailable"),
        control.clone(),
    )
    .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let storage = owner.start().unwrap().ready().await.unwrap();
    owner.request_stop();
    gate.arrived().await;
    control.state_unavailable_once();
    gate.release();
    assert!(!host.observe_actual().await);
    let snapshot = owner.state.test_snapshot();
    assert!(snapshot.roots_retained);
    assert_eq!(snapshot.actual, Actual::Unconfirmed);
    assert!(snapshot.scope.unwrap().all_threads_terminated());
    assert!(
        snapshot
            .joins
            .iter()
            .any(|join| join.state == AppJoinState::Unconfirmed && !join.task_join_error)
    );
    drop(storage);
}

#[tokio::test]
async fn dev_original_blocking_panic_before_claim_keeps_vendor_handle_and_roots() {
    let control = TestControl::default();
    let gate = control.pause_next(TestPoint::AppJoinHandleInstalledBeforeGate);
    let owner = StorageOpenOwner::prepare_for_test(
        &Handle::current(),
        roots("panic-before-claim"),
        control.clone(),
    )
    .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let storage = owner.start().unwrap().ready().await.unwrap();
    owner.request_stop();
    gate.arrived().await;
    control.panic_next_blocking_before_claim();
    gate.release();
    assert!(!host.observe_actual().await);
    let snapshot = owner.state.test_snapshot();
    assert!(snapshot.roots_retained);
    assert_eq!(snapshot.actual, Actual::Unconfirmed);
    assert!(
        snapshot
            .scope
            .unwrap()
            .workers
            .iter()
            .any(|worker| worker.handle == WorkerHandleState::VendorOwned)
    );
    assert!(
        snapshot
            .joins
            .iter()
            .any(|join| join.task_join_error && join.state == AppJoinState::Unconfirmed)
    );
    drop(storage);
}

async fn unknown_schema_roots(label: &str) -> StorageOpenRoots {
    use sqlx::Connection;
    let original = Arc::new(roots(label));
    let path = original.path.clone();
    let owner = StorageOpenOwner::prepare(
        &Handle::current(),
        StorageOpenRoots::retained(&path, original.clone()),
    )
    .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    owner
        .start_worker_scope()
        .unwrap()
        .preflight_ready()
        .await
        .unwrap();
    let options = owner.connection_options().unwrap().create_if_missing(true);
    let mut connection = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    sqlx::query("PRAGMA user_version=77")
        .execute(&mut connection)
        .await
        .unwrap();
    connection.close().await.unwrap();
    owner.request_stop();
    assert!(host.observe_actual().await);
    StorageOpenRoots::retained(&path, original)
}

#[tokio::test]
async fn dev_repeated_failed_preflight_never_turns_into_success() {
    let owner = StorageOpenOwner::prepare(
        &Handle::current(),
        unknown_schema_roots("repeated-error").await,
    )
    .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let ticket = owner.start_preflight().unwrap();
    let first = ticket.preflight_ready().await.unwrap_err();
    assert!(matches!(first.as_ref(), StorageError::UnknownSchema(77)));
    let second = ticket
        .preflight_ready()
        .await
        .expect_err("same failed preflight must remain an error");
    assert!(matches!(second.as_ref(), StorageError::UnknownSchema(77)));
    assert!(Arc::ptr_eq(&first, &second));
    let (one, two, storage) = tokio::join!(
        ticket.preflight_ready(),
        ticket.preflight_ready(),
        ticket.ready()
    );
    for observed in [one.unwrap_err(), two.unwrap_err(), storage.err().unwrap()] {
        assert!(Arc::ptr_eq(&first, &observed));
        assert!(matches!(observed.as_ref(), StorageError::UnknownSchema(77)));
    }
    assert!(host.observe_actual().await);
}

#[tokio::test]
async fn dev_stop_wins_original_admission_and_prohibits_late_driver() {
    let control = TestControl::default();
    let barrier = control.pause_next(TestPoint::StartBeforeAdmission);
    let owner = StorageOpenOwner::prepare_for_test(
        &Handle::current(),
        roots("stop-wins-admission"),
        control,
    )
    .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let starter = {
        let owner = owner.clone();
        std::thread::spawn(move || owner.start_worker_scope())
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), barrier.arrived())
        .await
        .unwrap();
    let stopper = {
        let owner = owner.clone();
        std::thread::spawn(move || owner.request_stop())
    };
    stopper.join().unwrap();
    assert!(host.observe_actual().await);
    let closed_scope = owner.state.test_snapshot();
    assert_eq!(closed_scope.actual, Actual::Joined);
    assert!(!closed_scope.roots_retained);
    assert_eq!(closed_scope.driver.handle_installed, Some(false));
    barrier.release();
    assert!(
        starter.join().unwrap().is_err(),
        "stop-first completion must forbid any later driver admission"
    );
    let final_state = owner.state.test_snapshot();
    assert_eq!(final_state.driver.handle_installed, Some(false));
    assert!(final_state.scope.unwrap().workers.is_empty());
}

#[tokio::test]
async fn dev_start_wins_admission_keeps_roots_until_original_driver_join() {
    let control = TestControl::default();
    let admitted = control.pause_next(TestPoint::StartAdmittedBeforeDriverInstall);
    let driver_gate = control.pause_next(TestPoint::ParentInstalledBeforeDriverGate);
    let owner = StorageOpenOwner::prepare_for_test(
        &Handle::current(),
        roots("start-wins-admission"),
        control,
    )
    .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let starter = {
        let owner = owner.clone();
        std::thread::spawn(move || owner.start_worker_scope())
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), admitted.arrived())
        .await
        .unwrap();
    let stopper = {
        let owner = owner.clone();
        std::thread::spawn(move || owner.request_stop())
    };
    stopper.join().unwrap();
    let starting = owner.state.test_snapshot();
    assert!(starting.roots_retained);
    assert_eq!(starting.driver.handle_installed, Some(false));
    assert_ne!(starting.actual, Actual::Joined);
    admitted.release();
    let ticket = starter.join().unwrap().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), driver_gate.arrived())
        .await
        .unwrap();
    let installed = owner.state.test_snapshot();
    assert!(installed.roots_retained);
    assert_eq!(installed.driver.handle_installed, Some(true));
    assert!(!installed.driver.gate_open);
    driver_gate.release();
    assert!(host.observe_actual().await);
    let joined = owner.state.test_snapshot();
    assert!(!joined.roots_retained);
    assert_eq!(joined.driver.handle_installed, Some(false));
    assert_eq!(joined.open_operation, starting.open_operation);
    drop(ticket);
}

#[tokio::test]
async fn dev_repeated_storage_ready_keeps_original_success_identity() {
    let owner = StorageOpenOwner::prepare(&Handle::current(), roots("repeated-success")).unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let ticket = owner.start().unwrap();
    let first = ticket.ready().await.unwrap();
    let (one, two) = tokio::join!(ticket.ready(), ticket.ready());
    assert!(Arc::ptr_eq(&first, &one.unwrap()));
    assert!(Arc::ptr_eq(&first, &two.unwrap()));
    owner.request_stop();
    assert!(host.observe_actual().await);
}

#[tokio::test]
async fn dev_repeated_failed_storage_open_keeps_original_error_identity() {
    let owner = StorageOpenOwner::prepare(
        &Handle::current(),
        unknown_schema_roots("repeated-open-error").await,
    )
    .unwrap();
    let host = StorageLifecycleHost::install(owner.clone());
    let ticket = owner.start().unwrap();
    let first = ticket.ready().await.err().unwrap();
    let second = ticket.ready().await.err().unwrap();
    assert!(matches!(first.as_ref(), StorageError::UnknownSchema(77)));
    assert!(Arc::ptr_eq(&first, &second));
    let (one, two) = tokio::join!(ticket.ready(), ticket.ready());
    for error in [one.err().unwrap(), two.err().unwrap()] {
        assert!(Arc::ptr_eq(&first, &error));
        assert!(matches!(error.as_ref(), StorageError::UnknownSchema(77)));
    }
    assert!(host.observe_actual().await);
}
