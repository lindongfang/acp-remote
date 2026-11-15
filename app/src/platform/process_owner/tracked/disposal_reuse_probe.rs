//! Author-only causal probe. Every handle here comes from a real original acquisition.
use super::*;
use std::future::Future;
use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};

fn missing_spec() -> ProcessSpec {
    let spec = ProcessSpec {
        executable: std::env::temp_dir().join("native-author-owned-missing-3b5d8632.exe"),
        cwd: std::env::current_dir().unwrap(),
        args: vec![],
        env: vec![],
    };
    assert!(!spec.executable.exists());
    spec
}

#[tokio::test]
async fn actual_pending_connect_and_driver_cancellation_keep_original_setup_until_borrow_ends() {
    let before_child = Arc::new(crate::test_support::AsyncBarrier::default());
    let (mut owner, driver) = OwnedProcess::prepare_tracked(&missing_spec())
        .unwrap()
        .into_owner_driver();
    *owner.shared.before_child.lock().unwrap() = Some(before_child.clone());
    let task = tokio::spawn(driver.run(Instant::now() + Duration::from_secs(5)));
    tokio::time::timeout(Duration::from_secs(2), before_child.wait_until_entered())
        .await
        .unwrap();
    let server = owner.shared.state.lock().unwrap().pipes[0]
        .server
        .as_ref()
        .unwrap()
        .clone();
    let mut connection = Box::pin(server.connect());
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(connection.as_mut().poll(cx).is_pending())).await
    );
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    assert!(matches!(
        owner
            .shutdown(Instant::now() + Duration::from_millis(40))
            .await,
        TrackedShutdownReport::Unconfirmed
    ));
    {
        let state = owner.shared.state.lock().unwrap();
        assert!(
            !state.pipes[0].closed && state.pipes[0].identity_reference.as_ref().unwrap().owned
        );
        assert!(!state.created && state.never_created.is_none());
        drop(state);
    }
    drop(connection);
    drop(server);
    let TrackedShutdownReport::NeverCreated(proof) = owner
        .shutdown(Instant::now() + Duration::from_secs(3))
        .await
    else {
        panic!("same real pending-connect setup did not close after its borrow/future ended");
    };
    assert!(owner.confirms_never_created(&proof));
    assert!(
        owner
            .shared
            .state
            .lock()
            .unwrap()
            .pipes
            .iter()
            .all(|pipe| pipe.closed && pipe.identity_reference.as_ref().unwrap().closed)
    );
}

#[tokio::test]
async fn actual_same_kernel_pipe_identity_stays_pending_until_real_disposal() {
    let prepared = OwnedProcess::prepare_tracked(&missing_spec())
        .unwrap()
        .with_hooks_for_test(TrackedSpawnHooks {
            cut: Some(TrackedSpawnCut::AfterFirstPipe),
            before_create: None,
        });
    let (mut owner, driver) = prepared.into_owner_driver();
    assert!(matches!(
        driver.run(Instant::now() + Duration::from_secs(3)).await,
        ProcessSpawnCompletion::Failed(_)
    ));
    {
        let state = owner.shared.state.lock().unwrap();
        assert_eq!(state.pipes.len(), 1);
        let pipe = &state.pipes[0];
        assert!(matches!(
            pipe.observe_original_identity(),
            OriginalPipeObservation::StillOriginal
        ));
        assert!(!pipe.closed && pipe.original_retired.is_none());
        assert!(pipe.identity_reference.as_ref().unwrap().owned);
        let mut flags = 0;
        // SAFETY: Read only the actual still-owned identity reference while its State lock is held.
        assert_ne!(
            unsafe {
                GetHandleInformation(
                    pipe.identity_reference.as_ref().unwrap().handle(),
                    &raw mut flags,
                )
            },
            0
        );
        assert_eq!(
            flags & windows_sys::Win32::Foundation::HANDLE_FLAG_INHERIT,
            0
        );
        drop(state);
    }
    assert!(matches!(
        owner
            .shutdown(Instant::now() + Duration::from_secs(3))
            .await,
        TrackedShutdownReport::NeverCreated(_)
    ));
    assert!(
        owner
            .shared
            .state
            .lock()
            .unwrap()
            .pipes
            .iter()
            .all(|pipe| pipe.closed && pipe.identity_reference.as_ref().unwrap().closed)
    );
}

#[tokio::test]
async fn actual_duplicate_failure_cut_retains_and_explicitly_closes_original_owned_pipe() {
    let (mut owner, driver) = OwnedProcess::prepare_tracked(&missing_spec())
        .unwrap()
        .into_owner_driver();
    *owner.shared.pipe_fault.lock().unwrap() = Some(PipeFault::DuplicateCall);
    assert!(matches!(
        driver.run(Instant::now() + Duration::from_secs(3)).await,
        ProcessSpawnCompletion::Failed(ProcessOwnerError::InvalidSpec(
            "synthetic before pipe duplicate"
        ))
    ));
    {
        let state = owner.shared.state.lock().unwrap();
        assert_eq!(state.pipes.len(), 1);
        assert!(
            state.pipes[0]
                .original_before_registration
                .as_ref()
                .unwrap()
                .owned
        );
        assert!(state.pipes[0].identity_reference.is_none() && !state.pipes[0].closed);
        assert!(!state.create_entered && state.blocking.is_none());
        drop(state);
    }
    assert!(matches!(
        owner
            .shutdown(Instant::now() + Duration::from_secs(3))
            .await,
        TrackedShutdownReport::NeverCreated(_)
    ));
    let state = owner.shared.state.lock().unwrap();
    assert!(
        state.pipes[0]
            .original_before_registration
            .as_ref()
            .unwrap()
            .closed
    );
    assert!(state.pipes[0].closed && state.pipes[0].identity_reference.is_none());
    drop(state);
}

#[tokio::test]
async fn actual_missing_create_observation_and_identity_close_faults_do_not_grant_proof() {
    for fault in [PipeFault::ObjectObservation, PipeFault::IdentityClose] {
        let (mut owner, driver) = OwnedProcess::prepare_tracked(&missing_spec())
            .unwrap()
            .into_owner_driver();
        *owner.shared.pipe_fault.lock().unwrap() = Some(fault);
        assert!(matches!(
            driver.run(Instant::now() + Duration::from_secs(3)).await,
            ProcessSpawnCompletion::Failed(ProcessOwnerError::Windows {
                operation: "CreateProcessW",
                code: 2
            })
        ));
        assert!(matches!(
            owner
                .shutdown(Instant::now() + Duration::from_millis(60))
                .await,
            TrackedShutdownReport::Unconfirmed
        ));
        {
            let mut state = owner.shared.state.lock().unwrap();
            assert!(state.never_created.is_none() && state.suspended.is_none());
            assert!(
                !state.pipes[0].closed && state.pipes[0].identity_reference.as_ref().unwrap().owned
            );
            if fault == PipeFault::IdentityClose {
                assert!(state.pipes[0].original_retired.is_some());
            }
            state.pipes[0].fault = None;
        }
        let TrackedShutdownReport::NeverCreated(first) = owner
            .shutdown(Instant::now() + Duration::from_secs(3))
            .await
        else {
            panic!("same original setup not closed after fault cut released");
        };
        let TrackedShutdownReport::NeverCreated(repeated) = owner
            .shutdown(Instant::now() + Duration::from_secs(3))
            .await
        else {
            panic!("cached proof lost");
        };
        assert!(Arc::ptr_eq(&first, &repeated));
        assert!(
            owner
                .shared
                .state
                .lock()
                .unwrap()
                .pipes
                .iter()
                .all(|pipe| pipe.closed && pipe.identity_reference.as_ref().unwrap().closed)
        );
    }
}

#[tokio::test]
async fn actual_running_handoff_reference_close_failure_keeps_same_running_cleanup_owner() {
    let executable =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/cmd.exe");
    let spec = ProcessSpec {
        executable,
        args: ["/d", "/c", "exit 0"]
            .into_iter()
            .map(OsString::from)
            .collect(),
        cwd: std::env::current_dir().unwrap(),
        env: vec![],
    };
    let (mut owner, driver) = OwnedProcess::prepare_tracked(&spec)
        .unwrap()
        .into_owner_driver();
    *owner.shared.pipe_fault.lock().unwrap() = Some(PipeFault::IdentityClose);
    assert!(matches!(
        driver.run(Instant::now() + Duration::from_secs(5)).await,
        ProcessSpawnCompletion::Failed(ProcessOwnerError::InvalidSpec(
            "pipe identity handoff closure unconfirmed"
        ))
    ));
    {
        let state = owner.shared.state.lock().unwrap();
        assert!(state.running.is_some() && state.created && state.assigned && state.resumed);
        assert!(
            state
                .pipes
                .iter()
                .all(|pipe| pipe.transferred && !pipe.closed)
        );
        assert!(state.pipes[0].identity_reference.as_ref().unwrap().owned);
        drop(state);
    }
    assert!(owner.take_io().is_err());
    assert!(matches!(
        owner
            .shutdown(Instant::now() + Duration::from_millis(300))
            .await,
        TrackedShutdownReport::Unconfirmed
    ));
    owner.shared.state.lock().unwrap().pipes[0].fault = None;
    let TrackedShutdownReport::AssignedJob(report) = owner
        .shutdown(Instant::now() + Duration::from_secs(3))
        .await
    else {
        panic!("original running owner/reference cleanup not confirmed");
    };
    assert_eq!(report.active_processes, 0);
    let state = owner.shared.state.lock().unwrap();
    assert!(
        state
            .pipes
            .iter()
            .all(|pipe| pipe.identity_closed_after_handoff() && !pipe.closed)
    );
    assert!(state.never_created.is_none() && state.suspended.is_none());
    drop(state);
}

#[tokio::test]
async fn actual_missing_create_reused_disposed_pipe_number_does_not_block_original_owner() {
    let spec = ProcessSpec {
        executable: std::env::temp_dir().join("native-author-owned-missing-3b5d8632.exe"),
        cwd: std::env::current_dir().unwrap(),
        args: vec![],
        env: vec![],
    };
    assert!(!spec.executable.exists());
    let prepared = OwnedProcess::prepare_tracked(&spec).unwrap();
    let (mut owner, driver) = prepared.into_owner_driver();
    assert!(matches!(
        driver.run(Instant::now() + Duration::from_secs(10)).await,
        ProcessSpawnCompletion::Failed(ProcessOwnerError::Windows {
            operation: "CreateProcessW",
            code: 2
        })
    ));
    let original_raw = {
        let mut state = owner.shared.state.lock().unwrap();
        assert!(state.create_rejected && !state.created && state.pipes.len() == 3);
        let pipe = &mut state.pipes[0];
        assert!(pipe.dispose_original_server());
        assert!(pipe.disposed && !pipe.closed && !pipe.transferred);
        let raw = pipe.raw;
        drop(state);
        raw
    };
    let mut events = Vec::new();
    events.try_reserve_exact(4096).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut reused = false;
    while events.len() < 4096 && Instant::now() < deadline {
        // SAFETY: This creates a new private unnamed Event. It never operates on the disposed number.
        let event = own(
            unsafe { CreateEventW(null(), 0, 0, null()) },
            "author event",
        )
        .unwrap();
        let same_numeric_value = event.as_raw_handle() as usize == original_raw;
        events.push(event);
        if same_numeric_value {
            reused = true;
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        reused,
        "actual OS Event allocation did not reach the bounded reuse cut"
    );
    let mut flags = 0;
    // SAFETY: Read-only check; this number is now the still-owned new Event, not the old pipe.
    assert_ne!(
        unsafe { GetHandleInformation(original_raw as HANDLE, &raw mut flags) },
        0
    );
    let result = owner
        .shutdown(Instant::now() + Duration::from_millis(300))
        .await;
    let expected = matches!(&result, TrackedShutdownReport::NeverCreated(proof) if owner.confirms_never_created(proof));
    let (different_object_observed, references_closed_while_event_alive) = {
        let state = owner.shared.state.lock().unwrap();
        (
            matches!(
                state.pipes[0].original_retired,
                Some(OriginalPipeRetirement::DifferentKernelObject)
            ),
            state.pipes.iter().all(|pipe| {
                pipe.closed
                    && pipe
                        .identity_reference
                        .as_ref()
                        .is_some_and(|reference| reference.closed)
            }),
        )
    };
    let reused_event = events
        .iter()
        .find(|event| event.as_raw_handle() as usize == original_raw)
        .unwrap();
    let mut event_flags = 0;
    // SAFETY: This is the actual newly acquired Event still owned by this test,
    // independently of the source's disposed original pipe number.
    let event_valid_after_shutdown = unsafe {
        GetHandleInformation(reused_event.as_raw_handle() as HANDLE, &raw mut event_flags)
    } != 0;
    // SAFETY: Operate only on that real still-owned synthetic Event, never on an
    // original disposed pipe or a supplied handle value.
    let event_usable_after_shutdown =
        unsafe { SetEvent(reused_event.as_raw_handle() as HANDLE) } != 0;
    // Always release only these real new Event owners and finish the actual original owner before the assertion.
    drop(events);
    let cleanup = owner
        .shutdown(Instant::now() + Duration::from_secs(3))
        .await;
    assert!(
        matches!(&cleanup, TrackedShutdownReport::NeverCreated(proof) if owner.confirms_never_created(proof))
    );
    assert!(
        expected,
        "original pipe was physically closed and its number reused by a real owned Event, but the old observer stayed Unconfirmed"
    );
    assert!(different_object_observed && references_closed_while_event_alive);
    assert!(event_valid_after_shutdown && event_usable_after_shutdown);
}
