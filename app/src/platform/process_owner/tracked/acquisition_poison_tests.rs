//! Actual acquisition and mutex-poison regression; no supplied closure observation or OS handle value.
use super::*;

fn prepared() -> (TrackedProcessOwner, ProcessSpawnDriver) {
    let spec = ProcessSpec {
        executable: std::env::temp_dir().join("native-author-poison-missing-123c454a.exe"),
        cwd: std::env::current_dir().unwrap(),
        args: vec![],
        env: vec![],
    };
    assert!(!spec.executable.exists());
    OwnedProcess::prepare_tracked(&spec)
        .unwrap()
        .into_owner_driver()
}
fn poison(shared: &Arc<Shared>) {
    let result = std::panic::catch_unwind(|| {
        let _guard = shared.state.lock().unwrap();
        panic!("synthetic exact mutex poison");
    });
    assert!(result.is_err() && shared.state.is_poisoned());
}
fn inventory(shared: &Arc<Shared>) -> (usize, usize, bool) {
    let state = shared
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let result = (
        state.pipes.len(),
        state.children.len(),
        state.pipes.iter().all(|p| p.closed),
    );
    drop(state);
    result
}
fn tracked_acquisitions(shared: &Arc<Shared>) -> bool {
    let (pipes, children, _) = inventory(shared);
    pipes == shared.acquired_pipes.load(Ordering::SeqCst)
        && children == shared.acquired_children.load(Ordering::SeqCst)
}

#[tokio::test]
async fn poisoned_before_acquisition_cannot_grant_setup_with_unrecorded_actual_handle() {
    let (mut owner, driver) = prepared();
    poison(&owner.shared);
    assert!(matches!(
        driver.run(Instant::now() + Duration::from_secs(3)).await,
        ProcessSpawnCompletion::Failed(_)
    ));
    let report = owner
        .shutdown(Instant::now() + Duration::from_secs(3))
        .await;
    assert!(
        !matches!(report, TrackedShutdownReport::NeverCreated(_))
            || tracked_acquisitions(&owner.shared),
        "original setup proof omitted an actual acquired pipe after mutex poison"
    );
    assert!(
        owner.shared.state.is_poisoned()
            && tracked_acquisitions(&owner.shared)
            && inventory(&owner.shared).2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn acquisition_record_poison_and_shutdown_stop_keep_original_handle_before_proof() {
    let (mut owner, driver) = prepared();
    let barrier = Arc::new(TrackedSpawnBarrier::default());
    *owner.shared.after_pipe_create.lock().unwrap() = Some(barrier.clone());
    let task = tokio::spawn(driver.run(Instant::now() + Duration::from_secs(5)));
    tokio::time::timeout(Duration::from_secs(2), barrier.wait_until_entered())
        .await
        .unwrap();
    poison(&owner.shared);
    let early = owner
        .shutdown(Instant::now() + Duration::from_millis(20))
        .await;
    barrier.release();
    assert!(matches!(
        task.await.unwrap(),
        ProcessSpawnCompletion::Failed(_)
    ));
    let report = owner
        .shutdown(Instant::now() + Duration::from_secs(3))
        .await;
    assert!(matches!(early, TrackedShutdownReport::Unconfirmed));
    assert!(
        !matches!(report, TrackedShutdownReport::NeverCreated(_))
            || tracked_acquisitions(&owner.shared),
        "cancellation/poison allowed proof without actual acquired handle in original inventory"
    );
    assert!(owner.shared.state.is_poisoned() && tracked_acquisitions(&owner.shared));
    assert!(inventory(&owner.shared).2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poison_after_original_record_keeps_real_owned_pipe_until_explicit_close() {
    let (mut owner, driver) = prepared();
    let barrier = Arc::new(TrackedSpawnBarrier::default());
    *owner.shared.after_pipe_record.lock().unwrap() = Some(barrier.clone());
    let task = tokio::spawn(driver.run(Instant::now() + Duration::from_secs(5)));
    tokio::time::timeout(Duration::from_secs(2), barrier.wait_until_entered())
        .await
        .unwrap();
    assert_eq!(inventory(&owner.shared).0, 1);
    poison(&owner.shared);
    barrier.release();
    assert!(matches!(
        task.await.unwrap(),
        ProcessSpawnCompletion::Failed(_)
    ));
    assert!(matches!(
        owner
            .shutdown(Instant::now() + Duration::from_secs(3))
            .await,
        TrackedShutdownReport::NeverCreated(_)
    ));
    assert!(
        owner.shared.state.is_poisoned()
            && tracked_acquisitions(&owner.shared)
            && inventory(&owner.shared).2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poison_after_server_registration_cannot_drop_unrecorded_actual_child_handle() {
    let (mut owner, driver) = prepared();
    let after_create = Arc::new(TrackedSpawnBarrier::default());
    *owner.shared.after_child_create.lock().unwrap() = Some(after_create.clone());
    let mut task = tokio::spawn(driver.run(Instant::now() + Duration::from_secs(5)));
    tokio::select! {
        result = &mut task => {
            let result = match result {
                Ok(ProcessSpawnCompletion::Running(_)) => "running".to_owned(),
                Ok(ProcessSpawnCompletion::Failed(error)) => error.to_string(),
                Err(error) => error.to_string(),
            };
            panic!("driver completed before child acquisition: {result}");
        }
        result = tokio::time::timeout(Duration::from_secs(2), after_create.wait_until_entered()) => {
            result.expect("child acquisition barrier timed out");
        }
    }
    assert_eq!(owner.shared.acquired_children.load(Ordering::SeqCst), 1);
    poison(&owner.shared);
    after_create.release();
    assert!(matches!(
        task.await.unwrap(),
        ProcessSpawnCompletion::Failed(_)
    ));
    let report = owner
        .shutdown(Instant::now() + Duration::from_secs(3))
        .await;
    assert!(
        !matches!(report, TrackedShutdownReport::NeverCreated(_))
            || tracked_acquisitions(&owner.shared),
        "original setup proof omitted an actual acquired child after mutex poison"
    );
    assert!(tracked_acquisitions(&owner.shared));
    assert!(owner.shared.state.is_poisoned() && inventory(&owner.shared).2);
}
