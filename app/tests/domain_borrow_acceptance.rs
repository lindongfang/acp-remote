//! DBR public behavior preparation. Execution requires the implementation manifest.
#[path = "acceptance/domain_borrow_support.rs"]
mod support;
use acp_remote::domain::views::StagedItem;
use std::{
    cell::Cell,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use support::*;

#[test]
fn dbr01_empty_finished_incomplete_and_exact_iterator() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    let before = windows.usage();
    let empty = windows
        .with_window(&handle, |guard| {
            assert_eq!(guard.status().state, ViewState::Reading);
            assert_eq!(guard.items().len(), 0);
            (guard.status(), guard.stamp())
        })
        .unwrap();
    assert_eq!(empty.0.retained_items, 0);
    assert_eq!(empty.1.revision(), 0);
    assert_eq!(windows.usage(), before);
    windows.insert(&handle, Some(key("a")), b"one").unwrap();
    windows.insert(&handle, None, b"two").unwrap();
    windows
        .with_window(&handle, |guard| {
            let mut items = guard.items();
            assert_eq!(items.len(), 2);
            let first = items.next().unwrap();
            assert_eq!(first.position, 1);
            assert_eq!(first.key.unwrap().id, "a");
            assert_eq!(first.bytes, b"one");
            assert_eq!(items.len(), 1);
            let second = items.next().unwrap();
            assert_eq!(second.position, 2);
            assert!(second.key.is_none());
            assert_eq!(second.bytes, b"two");
            assert_eq!(items.len(), 0);
        })
        .unwrap();
    windows.finish(&handle).unwrap();
    assert_eq!(
        windows.with_window(&handle, |g| g.status().state).unwrap(),
        ViewState::Finished
    );
    windows.fail(&handle, ViewFailure::SourceRead).unwrap();
    assert_eq!(
        windows.with_window(&handle, |g| g.status().state).unwrap(),
        ViewState::Incomplete(ViewFailure::SourceRead)
    );
}

#[test]
fn dbr02_stamp_can_escape_as_control_metadata_and_handle_clone_validates() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    let original = stamp(&windows, &handle);
    let copied = original;
    let same_read = handle.clone();
    windows.validate_read_stamp(&same_read, &copied).unwrap();
    // The stamp borrows handle, not windows: a later exclusive mutation is permitted.
    windows.insert(&handle, None, b"body").unwrap();
    assert_eq!(
        windows.validate_read_stamp(&handle, &original),
        Err(ViewError::ReadChanged)
    );
    let _body_free_future = windows
        .with_window(&handle, |g| {
            let stamp = g.stamp();
            async move { stamp.revision() }
        })
        .unwrap();
}

#[test]
fn dbr03_complete_identity_and_old_scope_lifetime() {
    let (mut left, original) = fixture(ViewLimits::default());
    let (right, twin) = fixture(ViewLimits::default());
    let old_stamp = stamp(&left, &original);
    assert_eq!(original.generation(), twin.generation());
    assert_eq!(old_stamp.revision(), revision(&right, &twin));
    assert_eq!(
        right.validate_read_stamp(&twin, &old_stamp),
        Err(ViewError::Stale)
    );
    assert_eq!(
        right.validate_read_stamp(&original, &old_stamp),
        Err(ViewError::Stale)
    );
    let other_source = left.begin(target(10, "native-A"), epoch(3)).unwrap();
    assert_eq!(
        left.validate_read_stamp(&other_source, &old_stamp),
        Err(ViewError::Stale)
    );
    let other_native = left.begin(target(9, "native-B"), epoch(3)).unwrap();
    assert_eq!(
        left.validate_read_stamp(&other_native, &old_stamp),
        Err(ViewError::Stale)
    );
    left.release(&original).unwrap();
    let next = left.begin(target(9, "native-A"), epoch(4)).unwrap();
    assert_eq!(
        left.validate_read_stamp(&next, &old_stamp),
        Err(ViewError::Stale)
    );
    assert_eq!(
        left.validate_read_stamp(&original, &old_stamp),
        Err(ViewError::Stale)
    );
    // original is kept alive by old_stamp even after its container is destroyed.
    drop(left);
    let (replacement, new_handle) = fixture(ViewLimits::default());
    assert_eq!(
        replacement.validate_read_stamp(&new_handle, &old_stamp),
        Err(ViewError::Stale)
    );
}

struct OwnedChild(Option<Child>);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
fn child_with_watchdog(case: &str) {
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", case, "--nocapture", "--test-threads=1"])
        .env("ACPR_DBR_OWNED_CHILD", case)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut owner = OwnedChild(Some(child));
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = owner.0.as_mut().unwrap().try_wait().unwrap() {
            owner.0.take();
            assert!(status.success(), "owned child failed: {case}");
            return;
        }
        assert!(Instant::now() < until, "owned child deadlocked: {case}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
struct CleanupResult {
    stage: Option<StagedItem>,
    dropped: Arc<AtomicUsize>,
}
impl Drop for CleanupResult {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
        drop(self.stage.take());
    }
}

#[test]
fn dbr04_callback_drop_cross_thread_and_failed_r_cleanup() {
    const CASE: &str = "dbr04_callback_drop_cross_thread_and_failed_r_cleanup";
    if std::env::var("ACPR_DBR_OWNED_CHILD").ok().as_deref() != Some(CASE) {
        child_with_watchdog(CASE);
        return;
    }
    let (mut windows, handle) = fixture(ViewLimits::default());
    let pending = stage(&mut windows, &handle, None, b"unpublished");
    let calls = Cell::new(0);
    let result = windows.with_window(&handle, |guard| {
        calls.set(calls.get() + 1);
        let frozen = guard.status();
        assert_eq!(frozen.state, ViewState::Reading);
        drop(pending);
        assert_eq!(guard.status(), frozen);
        assert_eq!(
            windows.window_status(&handle).unwrap().state,
            ViewState::Incomplete(ViewFailure::Structure)
        );
        let _usage = windows.usage();
        windows
            .with_window(&handle, |inner| {
                assert_eq!(
                    inner.status().state,
                    ViewState::Incomplete(ViewFailure::Structure)
                );
            })
            .unwrap();
    });
    assert_eq!(result, Err(ViewError::ReadChanged));
    assert_eq!(calls.get(), 1);

    let (mut windows, handle) = fixture(ViewLimits::default());
    let pending = stage(&mut windows, &handle, None, b"cross-thread");
    let barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|scope| {
        let other = Arc::clone(&barrier);
        let worker = scope.spawn(move || {
            other.wait();
            drop(pending);
            other.wait();
        });
        let result = windows.with_window(&handle, |guard| {
            let frozen = guard.status();
            barrier.wait();
            barrier.wait();
            assert_eq!(guard.status(), frozen);
        });
        worker.join().unwrap();
        assert_eq!(result, Err(ViewError::ReadChanged));
    });

    let (mut windows, handle) = fixture(ViewLimits::default());
    let baseline = windows.usage();
    let first = stage(&mut windows, &handle, None, b"cause-read-change");
    let second = stage(&mut windows, &handle, None, b"r-destructor-reenters-cell");
    let dropped = Arc::new(AtomicUsize::new(0));
    let result = windows.with_window(&handle, |_| {
        drop(first);
        CleanupResult {
            stage: Some(second),
            dropped: Arc::clone(&dropped),
        }
    });
    assert!(matches!(result, Err(ViewError::ReadChanged)));
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(windows.usage(), baseline);
    // Already Incomplete: repeating a stage failure is a no-op, not ReadChanged.
    let pending = stage(&mut windows, &handle, None, b"no-op-failure");
    let before = revision(&windows, &handle);
    windows
        .with_window(&handle, |g| {
            drop(pending);
            assert_eq!(g.stamp().revision(), before);
        })
        .unwrap();
    assert_eq!(revision(&windows, &handle), before);
}

#[test]
fn dbr05_original_and_staged_changes_same_bytes_and_no_ops() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    assert_eq!(revision(&windows, &handle), 0);
    let work = windows.stage_workspace(&handle, 0).unwrap();
    drop(work);
    let snapshot = windows.snapshot(&handle).unwrap();
    drop(snapshot);
    assert_eq!(revision(&windows, &handle), 0);
    windows.insert(&handle, Some(key("a")), b"same").unwrap();
    assert_eq!(revision(&windows, &handle), 1);
    let lease = windows.item_lease(&handle, key("a")).unwrap();
    drop(lease);
    assert_eq!(revision(&windows, &handle), 1);
    windows.replace(&handle, key("a"), b"same").unwrap();
    assert_eq!(revision(&windows, &handle), 2);
    let staged = stage(&mut windows, &handle, Some(key("b")), b"B");
    assert_eq!(revision(&windows, &handle), 2);
    assert_eq!(windows.publish(&handle, staged).unwrap().position, 2);
    assert_eq!(revision(&windows, &handle), 3);
    let lease = windows.item_lease(&handle, key("a")).unwrap();
    let work = windows.stage_workspace(&handle, 0).unwrap();
    let mut staged = windows.stage_replace(&handle, work, lease, 4).unwrap();
    staged.parts().writer.write_all(b"same").unwrap();
    assert_eq!(windows.publish(&handle, staged).unwrap().position, 1);
    assert_eq!(revision(&windows, &handle), 4);
    windows.finish(&handle).unwrap();
    assert_eq!(revision(&windows, &handle), 5);
    windows.finish(&handle).unwrap();
    assert_eq!(revision(&windows, &handle), 5);
    windows.fail(&handle, ViewFailure::SourceRead).unwrap();
    assert_eq!(revision(&windows, &handle), 6);
    windows.fail(&handle, ViewFailure::Capacity).unwrap();
    assert_eq!(revision(&windows, &handle), 6);
    let pending = stage(&mut windows, &handle, None, b"no-op-drop");
    drop(pending);
    assert_eq!(revision(&windows, &handle), 6);
    assert_eq!(windows.finish(&handle), Err(ViewError::Incomplete));
    assert_eq!(
        windows.insert(&handle, Some(key("a")), b"duplicate"),
        Err(ViewError::Conflict)
    );
    assert_eq!(
        windows.replace(&handle, key("missing"), b"missing"),
        Err(ViewError::MissingItem)
    );
    assert_eq!(revision(&windows, &handle), 6);
    assert_eq!(
        windows.window_status(&handle).unwrap().state,
        ViewState::Incomplete(ViewFailure::SourceRead)
    );
}

#[test]
fn dbr05_error_fifo_is_versioned_and_leases_keep_actual_old_bodies() {
    for staged_path in [false, true] {
        let limits = ViewLimits {
            target_bytes: 1_000_000,
            node_bytes: 2_000_000,
            target_items: 4,
            node_items: 4,
        };
        let (mut windows, handle) = fixture(limits);
        windows.insert(&handle, Some(key("a")), b"A").unwrap();
        windows.insert(&handle, Some(key("b")), b"B").unwrap();
        let a = windows.item_lease(&handle, key("a")).unwrap();
        let b = windows.item_lease(&handle, key("b")).unwrap();
        let before = stamp(&windows, &handle);
        assert_eq!(before.revision(), 2);
        if staged_path {
            let work = windows.stage_workspace(&handle, 0).unwrap();
            assert!(matches!(
                windows.stage_insert(&handle, work, None, 1),
                Err(ViewError::Capacity)
            ));
        } else {
            assert_eq!(
                windows.insert(&handle, None, b"C"),
                Err(ViewError::Capacity)
            );
        }
        let status = windows.window_status(&handle).unwrap();
        assert_eq!(status.evicted_count, 2);
        assert_eq!(status.evicted_through, Some(2));
        assert_eq!(status.retained_items, 0);
        assert_eq!(revision(&windows, &handle), 5); // two real evictions + first Incomplete
        assert_eq!(
            windows.validate_read_stamp(&handle, &before),
            Err(ViewError::ReadChanged)
        );
        assert_eq!(a.item().bytes, b"A");
        assert_eq!(b.item().bytes, b"B");
        drop(a);
        drop(b);
        assert_eq!(windows.usage().items, 0);
        assert_eq!(revision(&windows, &handle), 5);
    }
}

#[test]
fn dbr05_overrun_short_write_and_failed_stage_are_single_visible_change() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    let work = windows.stage_workspace(&handle, 0).unwrap();
    let mut staged = windows.stage_insert(&handle, work, None, 1).unwrap();
    assert_eq!(
        staged.parts().writer.write_all(b"too-long"),
        Err(ViewError::WriteTooLong)
    );
    assert_eq!(revision(&windows, &handle), 1);
    assert!(matches!(
        windows.publish(&handle, staged),
        Err(ViewError::PoisonedStage)
    ));
    assert_eq!(revision(&windows, &handle), 1);
    let (mut windows, handle) = fixture(ViewLimits::default());
    let work = windows.stage_workspace(&handle, 0).unwrap();
    let staged = windows.stage_insert(&handle, work, None, 1).unwrap();
    assert!(matches!(
        windows.publish(&handle, staged),
        Err(ViewError::ShortWrite)
    ));
    assert_eq!(revision(&windows, &handle), 1);
}

#[test]
fn dbr06_post_point_and_old_generation_drop_do_not_promise_future_freshness() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    let pending = stage(&mut windows, &handle, None, b"later");
    let returned = windows
        .with_window(&handle, |guard| (guard.stamp(), pending))
        .unwrap();
    windows.validate_read_stamp(&handle, &returned.0).unwrap();
    drop(returned.1);
    assert_eq!(
        windows.validate_read_stamp(&handle, &returned.0),
        Err(ViewError::ReadChanged)
    );
    let old_stage = stage(&mut windows, &handle, None, b"old-generation");
    windows.release(&handle).unwrap();
    let next = windows.begin(target(9, "native-A"), epoch(4)).unwrap();
    let current = stamp(&windows, &next);
    windows.with_window(&next, |_| drop(old_stage)).unwrap();
    windows.validate_read_stamp(&next, &current).unwrap();
    assert_eq!(revision(&windows, &next), 0);
}

#[test]
fn dbr05_cas_workspace_and_replace_errors_version_the_first_failure() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    windows.insert(&handle, Some(key("a")), b"A").unwrap();
    let old = windows.item_lease(&handle, key("a")).unwrap();
    windows.replace(&handle, key("a"), b"A").unwrap();
    let before = stamp(&windows, &handle);
    let work = windows.stage_workspace(&handle, 0).unwrap();
    assert!(matches!(
        windows.stage_replace(&handle, work, old, 1),
        Err(ViewError::Conflict)
    ));
    assert_eq!(revision(&windows, &handle), before.revision() + 1);
    assert_eq!(
        windows.window_status(&handle).unwrap().state,
        ViewState::Incomplete(ViewFailure::Structure)
    );
    for overflow in [false, true] {
        let limits = if overflow {
            ViewLimits::default()
        } else {
            ViewLimits {
                target_bytes: 2500,
                node_bytes: 20000,
                target_items: 8,
                node_items: 8,
            }
        };
        let (mut windows, handle) = fixture(limits);
        let before = windows.usage();
        assert!(matches!(
            windows.stage_workspace(&handle, if overflow { usize::MAX } else { 0 }),
            Err(ViewError::Capacity)
        ));
        assert_eq!(windows.usage(), before);
        assert_eq!(revision(&windows, &handle), 1);
    }
    let (mut windows, handle) = fixture(ViewLimits {
        target_bytes: 4000,
        node_bytes: 20000,
        target_items: 8,
        node_items: 8,
    });
    windows.insert(&handle, Some(key("a")), b"A").unwrap();
    assert_eq!(
        windows.replace(&handle, key("a"), &vec![1; 4000]),
        Err(ViewError::Capacity)
    );
    assert_eq!(revision(&windows, &handle), 2);
    windows
        .with_window(&handle, |guard| {
            let item = guard.items().next().unwrap();
            assert_eq!(item.position, 1);
            assert_eq!(item.bytes, b"A");
            assert_eq!(guard.status().evicted_count, 0);
        })
        .unwrap();
}

fn stage(
    windows: &mut ViewWindows,
    handle: &ViewHandle,
    source: Option<SourceKey<'_>>,
    body: &[u8],
) -> StagedItem {
    let work = windows.stage_workspace(handle, 0).unwrap();
    let mut staged = windows
        .stage_insert(handle, work, source, body.len())
        .unwrap();
    staged.parts().writer.write_all(body).unwrap();
    staged
}
