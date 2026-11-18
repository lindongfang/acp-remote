use super::*;
use crate::{
    application::native_dispatch::{DispatchIoError, DispatchIoProgress},
    domain::native_dispatch::DispatchError,
};
use std::task::Poll;

fn native() -> NativeSessionKey {
    NativeSessionKey::from_config(Some(id(1)), "synthetic-native").unwrap()
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Matches the actual IO callback signature"
)]
fn accept() -> Result<Poll<Result<DispatchIoProgress, DispatchIoError>>, DispatchError> {
    Ok(Poll::Ready(Ok(DispatchIoProgress::Accepted(1))))
}

fn assert_busy(guard: &dyn AutomaticCloseGuard) {
    let mut invoked = false;
    let result = guard.poll_idle(id(200), &mut || {
        invoked = true;
        accept()
    });
    assert!(matches!(result, Err(DispatchError::NotReady)));
    assert!(!invoked);
}

#[tokio::test]
async fn automatic_close_tree_large_set_deduplicates_and_freezes_all_original_owners() {
    let fixture = Fixture::new().await;
    let mut natives: Vec<_> = (0..4096)
        .map(|number| {
            NativeSessionKey::from_config(Some(id(1)), &format!("child-{number}")).unwrap()
        })
        .collect();
    natives.push(natives[0].clone());
    let guard = fixture.driver.automatic_close_tree_guard(&natives).unwrap();
    natives.reverse();
    let reverse = fixture.driver.automatic_close_tree_guard(&natives).unwrap();
    let mut calls = 0;
    let _ = guard
        .poll_idle(id(200), &mut || {
            calls += 1;
            Ok(Poll::Pending)
        })
        .unwrap();
    assert_eq!(calls, 1);
    let _ = reverse.poll_idle(id(200), &mut accept).unwrap();
    assert_busy(guard.as_ref());
    for native in &natives {
        let single = fixture.driver.automatic_close_guard(native).unwrap();
        assert_busy(single.as_ref());
    }
    let _ = guard.poll_frozen(id(200), &mut accept).unwrap();
    assert!(!guard.finish(id(201)).unwrap());
    assert!(guard.finish(id(200)).unwrap());
    let _ = reverse.poll_idle(id(201), &mut accept).unwrap();
    assert!(
        !guard.finish(id(200)).unwrap(),
        "old operation cannot release the new tree freeze"
    );
    assert_busy(guard.as_ref());
    assert!(reverse.finish(id(201)).unwrap());
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_tree_revision_overflow_protects_before_io() {
    let fixture = Fixture::new().await;
    let first = native();
    let second = NativeSessionKey::from_config(Some(id(1)), "another-child").unwrap();
    let guard = fixture
        .driver
        .automatic_close_tree_guard(&[first.clone(), second.clone()])
        .unwrap();
    fixture
        .driver
        .core
        .owner_for(first)
        .unwrap()
        .state
        .lock()
        .unwrap()
        .native_order = u64::MAX;
    fixture
        .driver
        .core
        .owner_for(second)
        .unwrap()
        .state
        .lock()
        .unwrap()
        .native_order = 1;
    assert!(matches!(
        guard.poll_idle(id(200), &mut || panic!("overflow entered actual IO")),
        Err(DispatchError::SequenceExhausted)
    ));
    assert!(!guard.finish(id(200)).unwrap());
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_revision_retains_activity_between_two_idle_scans() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    let mut before = 0;
    let _ = guard
        .poll_idle_revision(id(200), &mut |revision| {
            before = revision;
            Ok(Poll::Pending)
        })
        .unwrap();
    let item = fixture.submit("brief-work-between-scans").await;
    fixture
        .apply(ValidatedMutation::Cancel {
            item: item.entity(),
            revision: 1,
        })
        .await;
    let mut after = before;
    let _ = guard
        .poll_idle_revision(id(200), &mut |revision| {
            after = revision;
            Ok(Poll::Pending)
        })
        .unwrap();
    assert!(
        after > before,
        "empty queue retains its recent Submit/Cancel generation"
    );
    assert!(!guard.finish(id(200)).unwrap());
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_empty_owner_installation_is_shared_without_target() {
    let fixture = Fixture::new().await;
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let mut handles = Vec::new();
    for index in 0..8 {
        let driver = fixture.driver.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            if index % 2 == 0 {
                let _owner = driver.core.owner_for(native()).unwrap();
            }
            driver.automatic_close_guard(&native()).unwrap()
        }));
    }
    let guards = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    {
        let owners = fixture.driver.core.owners.lock().unwrap();
        assert_eq!(owners.len(), 1);
        let owner = owners.get(&native()).unwrap();
        let state = owner.state.lock().unwrap();
        assert!(state.canonical_target.is_none());
        assert!(state.origins.is_empty());
    }
    assert!(guards[0].poll_idle(id(200), &mut accept).is_ok());
    for guard in guards {
        assert_busy(guard.as_ref());
    }
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_pending_then_input_withdraws_before_first_byte() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    assert!(matches!(
        guard.poll_idle(id(200), &mut || Ok(Poll::Pending)),
        Ok(Poll::Pending)
    ));
    fixture.submit("new-input-before-first-byte").await;
    assert_busy(guard.as_ref());
    assert!(!guard.finish(id(200)).unwrap());
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_nonaccepted_results_do_not_freeze() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    for result in [
        Ok(Poll::Pending),
        Ok(Poll::Ready(Ok(DispatchIoProgress::Accepted(0)))),
        Ok(Poll::Ready(Ok(DispatchIoProgress::Flushed))),
        Ok(Poll::Ready(Err(DispatchIoError))),
        Err(DispatchError::NotReady),
    ] {
        let mut result = Some(result);
        let _ = guard.poll_idle(id(200), &mut || result.take().unwrap());
        assert!(!guard.finish(id(200)).unwrap());
    }
    fixture.submit("zero-remains-available").await;
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_first_accepted_freezes_mutations_before_pending() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    let owner = fixture.driver.core.owner_for(native()).unwrap();
    assert!(
        guard
            .poll_idle(id(200), &mut || {
                assert!(matches!(
                    owner.state.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
                accept()
            })
            .is_ok()
    );
    for mutation in [
        fixture.mutation("frozen-input"),
        ValidatedMutation::Edit {
            item: id(90),
            revision: 1,
            body: fixture.service.validate_body("frozen-edit").unwrap(),
        },
        ValidatedMutation::Resume,
        ValidatedMutation::Prioritize {
            item: id(90),
            revision: 1,
        },
    ] {
        let registration = fixture
            .service
            .register(&id(10), id(TARGET), &mutation)
            .await
            .unwrap();
        assert!(matches!(
            fixture
                .service
                .consume(&id(10), id(TARGET), registration.id(), mutation)
                .await,
            ConsumeReply::Refused {
                error: RegistryError::Conflict,
                ..
            }
        ));
        assert!(matches!(
            fixture
                .service
                .query_request(&id(10), id(TARGET), registration.id())
                .await
                .unwrap(),
            RequestFact::Registered(_)
        ));
        assert!(owner.state.lock().unwrap().pending.is_none());
    }
    assert!(fixture.view().await.is_empty());
    assert!(guard.finish(id(200)).unwrap());
    fixture.submit("unfrozen-input").await;
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_waiting_suspended_prepared_and_active_protect() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    fixture.submit("waiting").await;
    assert_busy(guard.as_ref());
    let owner = fixture.driver.core.owner_for(native()).unwrap();
    owner.state.lock().unwrap().queue.suspend_waiting();
    assert_busy(guard.as_ref());
    fixture.apply(ValidatedMutation::Resume).await;
    fixture
        .environment
        .set_readiness(DispatchReadiness::Ready(epoch()));
    let prepared = fixture.driver.prepare(id(TARGET)).await.unwrap().unwrap();
    assert_busy(guard.as_ref());
    let active = fixture.driver.begin_selection(&prepared).await.unwrap();
    assert_busy(guard.as_ref());
    fixture
        .driver
        .observe(
            id(TARGET),
            QueueOwnerEvent::Settle {
                active,
                disposition: QueueDisposition::Advance,
            },
        )
        .await
        .unwrap();
    assert!(guard.poll_idle(id(200), &mut accept).is_ok());
    fixture.close().await;
}

struct ReleaseBarrier(Arc<Barrier>);
impl Drop for ReleaseBarrier {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[tokio::test]
async fn automatic_close_pending_necessary_operation_protects_empty_queue() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    let mutation = fixture.mutation("necessary-pending");
    let registration = fixture
        .service
        .register(&id(10), id(TARGET), &mutation)
        .await
        .unwrap();
    let barrier = Barrier::new();
    let release = ReleaseBarrier(barrier.clone());
    fixture
        .storage
        .request_registry()
        .faults_for_test()
        .arm(
            Point::AfterCommit(Operation::Begin),
            Action::Pause(barrier.clone()),
        )
        .unwrap();
    let service = fixture.service.clone();
    let task = tokio::spawn(async move {
        service
            .consume(&id(10), id(TARGET), registration.id(), mutation)
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), barrier.arrived())
        .await
        .unwrap();
    {
        let owner = fixture.driver.core.owner_for(native()).unwrap();
        let state = owner.state.lock().unwrap();
        assert!(state.pending.is_some());
        assert_eq!(state.queue.waiting().len(), 0);
    }
    assert_busy(guard.as_ref());
    drop(release);
    assert!(matches!(task.await.unwrap(), ConsumeReply::Resolved { .. }));
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_external_and_priority_markers_protect() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    let work = fixture
        .driver
        .open_external_work(id(TARGET), epoch())
        .await
        .unwrap();
    assert_busy(guard.as_ref());
    fixture
        .driver
        .observe(
            id(TARGET),
            QueueOwnerEvent::ExternalForegroundCompleted { work },
        )
        .await
        .unwrap();
    let owner = fixture.driver.core.owner_for(native()).unwrap();
    owner.state.lock().unwrap().selected_priority = Some((id(201), id(90), 1));
    assert_busy(guard.as_ref());
    {
        let mut state = owner.state.lock().unwrap();
        state.selected_priority = None;
        state.priority_handoff = Some(PriorityHandoff {
            identity: owner.identity.clone(),
            target: id(TARGET),
            item: id(90),
            revision: 1,
            resource: Some(epoch()),
            operation: id(201),
            registration: RegistrationId::new(id(202)),
        });
    }
    assert_busy(guard.as_ref());
    owner.state.lock().unwrap().priority_handoff = None;
    assert!(guard.poll_idle(id(200), &mut accept).is_ok());
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_unknown_keeps_original_fence_and_allows_observations() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    assert!(guard.poll_idle(id(200), &mut accept).is_ok());
    let owner = fixture.driver.core.owner_for(native()).unwrap();
    let resource = ResourceEpoch {
        runtime: owner.identity.runtime,
        ..epoch()
    };
    fixture.environment.set_resource(Some(resource));
    let link = native::observation_link(fixture.driver.core.clone(), owner);
    let work = link
        .apply(resource, &mut |_| Ok(NativeQueuePolicy::BeginExternal))
        .unwrap()
        .unwrap();
    assert!(matches!(
        guard.poll_frozen(id(200), &mut || Ok(Poll::Ready(Err(DispatchIoError)))),
        Ok(Poll::Ready(Err(_)))
    ));
    drop(guard);
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    assert_busy(guard.as_ref());
    assert!(guard.poll_frozen(id(200), &mut accept).is_ok());
    assert!(matches!(
        guard.poll_frozen(id(200), &mut || Ok(Poll::Ready(Ok(
            DispatchIoProgress::Flushed
        )))),
        Ok(Poll::Ready(Ok(DispatchIoProgress::Flushed)))
    ));
    fixture
        .driver
        .observe(
            id(TARGET),
            QueueOwnerEvent::ExternalForegroundCompleted { work },
        )
        .await
        .unwrap();
    assert_busy(guard.as_ref());
    assert!(guard.finish(id(200)).unwrap());
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_old_operation_cannot_release_or_write_new_operation() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    assert!(guard.poll_idle(id(200), &mut accept).is_ok());
    assert!(!guard.finish(id(201)).unwrap());
    assert_busy(guard.as_ref());
    assert!(guard.finish(id(200)).unwrap());
    assert!(!guard.finish(id(200)).unwrap());
    assert!(guard.poll_idle(id(201), &mut accept).is_ok());
    assert!(!guard.finish(id(200)).unwrap());
    let mut invoked = false;
    assert!(matches!(
        guard.poll_frozen(id(200), &mut || {
            invoked = true;
            accept()
        }),
        Err(DispatchError::StaleSelection)
    ));
    assert!(!invoked);
    assert_busy(guard.as_ref());
    assert!(guard.poll_frozen(id(201), &mut accept).is_ok());
    assert!(guard.finish(id(201)).unwrap());
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_close_first_byte_and_new_pending_are_mutually_exclusive() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    for _ in 0..16 {
        let mutation = fixture.mutation("concurrent-first-byte");
        let registration = fixture
            .service
            .register(&id(10), id(TARGET), &mutation)
            .await
            .unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let writer_barrier = barrier.clone();
        let writer_guard = guard.clone();
        let writer = std::thread::spawn(move || {
            writer_barrier.wait();
            writer_guard.poll_idle(id(200), &mut accept)
        });
        let service = fixture.service.clone();
        let submit_barrier = barrier.clone();
        let submit = tokio::spawn(async move {
            submit_barrier.wait();
            service
                .consume(&id(10), id(TARGET), registration.id(), mutation)
                .await
        });
        barrier.wait();
        let write = writer.join().unwrap();
        let consumed = submit.await.unwrap();
        match write {
            Ok(Poll::Ready(Ok(DispatchIoProgress::Accepted(1)))) => {
                assert!(matches!(
                    consumed,
                    ConsumeReply::Refused {
                        error: RegistryError::Conflict,
                        ..
                    }
                ));
                assert!(fixture.view().await.is_empty());
                assert!(guard.finish(id(200)).unwrap());
            }
            Err(DispatchError::NotReady) => {
                assert!(matches!(consumed, ConsumeReply::Resolved { .. }));
                let queued = fixture.view().await;
                assert_eq!(queued.len(), 1);
                fixture
                    .apply(ValidatedMutation::Cancel {
                        item: queued[0].0,
                        revision: queued[0].1,
                    })
                    .await;
            }
            _ => panic!("unexpected close/input race outcome"),
        }
    }
    fixture.close().await;
}

#[tokio::test]
async fn automatic_close_shutdown_blocks_poll_but_retains_owned_cleanup() {
    let fixture = Fixture::new().await;
    let guard = fixture.driver.automatic_close_guard(&native()).unwrap();
    assert!(guard.poll_idle(id(200), &mut accept).is_ok());
    let report = fixture
        .service
        .shutdown(RegistryDeadline::new(
            Instant::now() + Duration::from_secs(3),
        ))
        .await;
    assert!(report.joined);
    assert!(matches!(
        guard.poll_frozen(id(200), &mut accept),
        Err(DispatchError::NodeStopping)
    ));
    assert!(fixture.driver.automatic_close_guard(&native()).is_err());
    assert!(guard.finish(id(200)).unwrap());
    fixture.close().await;
}
