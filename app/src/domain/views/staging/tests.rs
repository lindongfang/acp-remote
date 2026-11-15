use super::*;
use crate::domain::{
    EntityId,
    native::{HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch},
    views::{KeyNamespace, ViewLimits, ViewUsage},
};
fn target(n: u8) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(EntityId::from_bytes([n; 16])), "native").unwrap()
}
fn epoch(n: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([1; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([2; 16])),
        load: LoadEpoch(EntityId::from_bytes([n; 16])),
    }
}
fn key(id: &str) -> SourceKey<'_> {
    SourceKey {
        namespace: KeyNamespace::Tool,
        id,
    }
}
fn views(bytes: usize, items: usize) -> ViewWindows {
    ViewWindows::new(ViewLimits {
        target_bytes: bytes,
        target_items: items,
        ..ViewLimits::default()
    })
    .unwrap()
}
fn write(stage: &mut StagedItem, bytes: &[u8]) {
    stage.parts().writer.write_all(bytes).unwrap();
}

#[test]
fn one_lease_is_charged_independently_and_only_its_object_survives_release() {
    let mut v = views(65536, 32);
    let h = v.begin(target(1), epoch(1)).unwrap();
    v.insert(&h, Some(key("a")), b"old").unwrap();
    v.insert(&h, Some(key("b")), b"other").unwrap();
    let before = v.target_usage(&target(1)).unwrap();
    let a = v.item_lease(&h, key("a")).unwrap();
    let b = v.item_lease(&h, key("a")).unwrap();
    assert_eq!(
        v.target_usage(&target(1)).unwrap(),
        ViewUsage {
            bytes: before.bytes + 640,
            items: 4
        }
    );
    assert!(matches!(
        v.item_lease(&h, key("missing")),
        Err(ViewError::MissingItem)
    ));
    assert_eq!(a.item().bytes, b"old");
    v.release(&h).unwrap();
    assert_eq!(
        v.target_usage(&target(1)).unwrap(),
        ViewUsage {
            bytes: 1030 + 516 + 640,
            items: 3
        }
    );
    drop(a);
    assert_eq!(v.target_usage(&target(1)).unwrap().items, 2);
    drop(b);
    v.collect_released();
    assert_eq!(v.identity_count(), 0);
    assert_eq!(
        v.usage(),
        ViewUsage {
            bytes: 8192,
            items: 0
        }
    );
}

#[test]
fn work_keeps_old_window_charge_and_does_not_poison_a_new_generation() {
    let mut v = views(20000, 8);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let mut work = v.stage_workspace(&h, 7680).unwrap();
    assert_eq!(work.scratch().len(), 7680);
    assert_eq!(v.target_usage(&target(1)).unwrap().bytes, 2054 + 8192);
    v.release(&h).unwrap();
    // The old status/window header remains paid even with no registry window.
    assert_eq!(v.target_usage(&target(1)).unwrap().bytes, 2054 + 8192);
    let new = v.begin(target(1), epoch(2)).unwrap();
    assert_eq!(
        v.target_usage(&target(1)).unwrap().bytes,
        2054 + 8192 + 1024
    );
    drop(work);
    assert_eq!(v.target_usage(&target(1)).unwrap().bytes, 2054);
    assert_eq!(v.window_status(&new).unwrap().state, ViewState::Reading);
    let work = v.stage_workspace(&new, 0).unwrap();
    assert_eq!(v.target_usage(&target(1)).unwrap().bytes, 2054 + 512);
    drop(work);
    assert_eq!(v.window_status(&new).unwrap().state, ViewState::Reading);
}

#[test]
fn complete_candidate_moves_its_allocation_and_preserves_old_snapshot() {
    let mut v = views(65536, 32);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let position = v.insert(&h, Some(key("a")), b"old").unwrap();
    v.insert(&h, None, b"later").unwrap();
    let snap = v.snapshot(&h).unwrap();
    let lease = v.item_lease(&h, key("a")).unwrap();
    let work = v.stage_workspace(&h, 7680).unwrap();
    let mut stage = v.stage_replace(&h, work, lease, 7).unwrap();
    let pointer = stage.data.as_ptr();
    {
        let mut parts = stage.parts();
        assert_eq!(parts.old.unwrap().bytes, b"old");
        parts.scratch[0] = 9;
        parts.writer.write_all(b"\0<").unwrap();
        assert_eq!(parts.writer.remaining(), 5);
        parts.writer.write_all(b"raw>\xff").unwrap();
        assert_eq!(parts.writer.written(), 7);
    }
    assert_eq!(v.window_status(&h).unwrap().retained_items, 2);
    let fact = v.publish(&h, stage).unwrap();
    assert!(fact.replaced);
    assert_eq!(fact.position, position);
    assert_eq!(fact.status.next_position, Some(3));
    let current = v.snapshot(&h).unwrap();
    assert_eq!(current.items().next().unwrap().bytes.as_ptr(), pointer);
    assert_eq!(current.items().next().unwrap().bytes, b"\0<raw>\xff");
    assert_eq!(snap.items().next().unwrap().bytes, b"old");
}

#[test]
fn overrun_is_zero_write_and_permanent_short_write_and_drop_remain_incomplete() {
    for operation in 0..3 {
        let mut v = views(65536, 16);
        let h = v.begin(target(1), epoch(1)).unwrap();
        let work = v.stage_workspace(&h, 0).unwrap();
        let mut stage = v.stage_insert(&h, work, None, 3).unwrap();
        write(&mut stage, b"ab");
        if operation == 0 {
            assert_eq!(
                stage.parts().writer.write_all(b"cd"),
                Err(ViewError::WriteTooLong)
            );
            assert_eq!(stage.parts().writer.written(), 2);
            assert_eq!(
                stage.parts().writer.write_all(b"c"),
                Err(ViewError::PoisonedStage)
            );
            assert_eq!(v.publish(&h, stage), Err(ViewError::PoisonedStage));
        } else if operation == 1 {
            assert_eq!(v.publish(&h, stage), Err(ViewError::ShortWrite));
        } else {
            drop(stage);
        }
        assert_eq!(v.window_status(&h).unwrap().retained_items, 0);
        assert_eq!(
            v.target_usage(&target(1)).unwrap(),
            ViewUsage {
                bytes: 2054,
                items: 0
            }
        );
        assert_eq!(v.finish(&h), Err(ViewError::Incomplete));
    }
}

#[test]
fn zero_length_publish_and_abort_are_distinct_and_status_needs_no_budget() {
    let mut v = views(3078, 1);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let work = v.stage_workspace(&h, 0).unwrap();
    let stage = v.stage_insert(&h, work, None, 0).unwrap();
    assert!(matches!(v.snapshot(&h), Err(ViewError::Capacity)));
    assert_eq!(v.window_status(&h).unwrap().retained_items, 0);
    assert_eq!(v.publish(&h, stage).unwrap().position, 1);
    assert_eq!(v.window_status(&h).unwrap().state, ViewState::Reading);
    let work = v.stage_workspace(&h, 0).unwrap();
    let stage = v.stage_insert(&h, work, None, 0).unwrap();
    v.abort_stage(&h, stage, ViewFailure::SourceRead).unwrap();
    assert_eq!(
        v.window_status(&h).unwrap().state,
        ViewState::Incomplete(ViewFailure::SourceRead)
    );
}

#[test]
fn two_old_leases_do_not_overwrite_a_new_item_or_aba_reinsertion() {
    let mut v = views(65536, 32);
    let h = v.begin(target(1), epoch(1)).unwrap();
    v.insert(&h, Some(key("a")), b"old").unwrap();
    let a = v.item_lease(&h, key("a")).unwrap();
    let b = v.item_lease(&h, key("a")).unwrap();
    let work = v.stage_workspace(&h, 0).unwrap();
    let mut first = v.stage_replace(&h, work, a, 1).unwrap();
    write(&mut first, b"1");
    let work = v.stage_workspace(&h, 0).unwrap();
    let mut second = v.stage_replace(&h, work, b, 1).unwrap();
    write(&mut second, b"2");
    v.publish(&h, first).unwrap();
    assert_eq!(v.publish(&h, second), Err(ViewError::Conflict));
    assert_eq!(v.item_lease(&h, key("a")).unwrap().item().bytes, b"1");
    let lease = v.item_lease(&h, key("a")).unwrap();
    let work = v.stage_workspace(&h, 0).unwrap();
    let mut stage = v.stage_replace(&h, work, lease, 1).unwrap();
    write(&mut stage, b"3");
    // Simulate a same-read FIFO eviction without invalidating the read generation.
    v.slot_mut(&h)
        .unwrap()
        .window
        .as_mut()
        .unwrap()
        .items
        .pop_front();
    v.insert(&h, Some(key("a")), b"1").unwrap();
    assert_eq!(v.publish(&h, stage), Err(ViewError::Conflict));
    assert_eq!(v.item_lease(&h, key("a")).unwrap().item().bytes, b"1");
}

#[test]
fn staged_drop_and_old_publish_only_fail_the_original_read_cell() {
    let mut v = views(65536, 16);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let work = v.stage_workspace(&h, 0).unwrap();
    let mut stage = v.stage_insert(&h, work, None, 1).unwrap();
    write(&mut stage, b"a");
    v.release(&h).unwrap();
    let new = v.begin(target(1), epoch(2)).unwrap();
    assert_eq!(v.publish(&new, stage), Err(ViewError::Stale));
    assert_eq!(v.window_status(&new).unwrap().state, ViewState::Reading);
    assert_eq!(v.target_usage(&target(1)).unwrap().bytes, 2054);
    let mut other = views(65536, 16);
    let foreign = other.begin(target(1), epoch(2)).unwrap();
    let work = v.stage_workspace(&new, 0).unwrap();
    assert!(matches!(
        other.stage_insert(&foreign, work, None, 1),
        Err(ViewError::Stale)
    ));
    assert_eq!(
        other.window_status(&foreign).unwrap().state,
        ViewState::Reading
    );
    assert_eq!(v.window_status(&new).unwrap().state, ViewState::Reading);
}

#[test]
fn multiple_unpublished_candidates_recheck_duplicate_keys_at_publish() {
    let mut v = views(65536, 16);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let w = v.stage_workspace(&h, 0).unwrap();
    let mut first = v.stage_insert(&h, w, Some(key("same")), 1).unwrap();
    let w = v.stage_workspace(&h, 0).unwrap();
    let mut second = v.stage_insert(&h, w, Some(key("same")), 1).unwrap();
    write(&mut first, b"a");
    write(&mut second, b"b");
    assert_eq!(v.publish(&h, first).unwrap().position, 1);
    assert_eq!(v.publish(&h, second), Err(ViewError::Conflict));
    assert_eq!(v.window_status(&h).unwrap().next_position, Some(2));
    assert_eq!(v.item_lease(&h, key("same")).unwrap().item().bytes, b"a");
}

#[test]
fn fifo_keeps_rolling_and_errors_preserve_eviction_facts_without_a_snapshot() {
    let mut v = views(4110, 3);
    let h = v.begin(target(1), epoch(1)).unwrap();
    for _ in 0..100 {
        let w = v.stage_workspace(&h, 0).unwrap();
        let mut stage = v.stage_insert(&h, w, None, 1).unwrap();
        write(&mut stage, b"x");
        v.publish(&h, stage).unwrap();
    }
    let status = v.window_status(&h).unwrap();
    assert_eq!(status.last_position, Some(100));
    assert!(status.evicted_count > 90);
    assert!(status.first_position.unwrap() > 90);
    assert!(matches!(v.snapshot(&h), Err(ViewError::Capacity)));
    let w = v.stage_workspace(&h, 0).unwrap();
    let staged = v.stage_insert(&h, w, None, 1).unwrap();
    let evicted = v.window_status(&h).unwrap().evicted_count;
    assert_eq!(v.publish(&h, staged), Err(ViewError::ShortWrite));
    assert_eq!(v.window_status(&h).unwrap().evicted_count, evicted);
    assert_eq!(v.finish(&h), Err(ViewError::Incomplete));
}

#[test]
fn replacement_peak_exact_boundary_and_checked_lengths_do_not_evict_old_values() {
    // B + old(516) + lease(320) + work(512) + candidate(516) = 3918.
    for max in [3917, 3918] {
        let mut v = views(max, 8);
        let h = v.begin(target(1), epoch(1)).unwrap();
        v.insert(&h, Some(key("a")), b"old").unwrap();
        let lease = v.item_lease(&h, key("a")).unwrap();
        let work = v.stage_workspace(&h, 0).unwrap();
        let result = v.stage_replace(&h, work, lease, 3);
        if max == 3917 {
            assert!(matches!(result, Err(ViewError::Capacity)));
            assert_eq!(v.window_status(&h).unwrap().evicted_count, 0);
            assert_eq!(v.item_lease(&h, key("a")).unwrap().item().bytes, b"old");
        } else {
            let mut stage = result.unwrap();
            assert_eq!(v.target_usage(&target(1)).unwrap().bytes, max);
            write(&mut stage, b"new");
            v.publish(&h, stage).unwrap();
            assert_eq!(v.target_usage(&target(1)).unwrap().bytes, 2054 + 516);
        }
        let w = v.stage_workspace(&h, 0).unwrap();
        assert!(matches!(
            v.stage_insert(&h, w, None, usize::MAX),
            Err(ViewError::Capacity)
        ));
        assert_eq!(v.window_status(&h).unwrap().evicted_count, 0);
    }
}

#[test]
fn actual_layout_and_bulk_capacity_stay_inside_precharged_headers() {
    use std::mem::size_of;
    println!(
        "workspace={} staged={} lease={} parts={} window={} status={}",
        size_of::<StageWorkspace>(),
        size_of::<StagedItem>(),
        size_of::<ItemLease>(),
        size_of::<StageParts<'_>>(),
        size_of::<Window>(),
        size_of::<ReadStatusCell>()
    );
    assert!(size_of::<StageWorkspace>() <= 512);
    assert!(size_of::<StagedItem>() + size_of::<StageParts<'_>>() <= 512);
    assert!(size_of::<ItemLease>() <= 256);
    assert!(size_of::<Window>() + size_of::<ReadStatusCell>() + 2 * size_of::<usize>() <= 1024);
    for n in [0, 1, 7, 511, 512, 4096, 7680, 65535] {
        let data = zeroed_exact(n).unwrap();
        assert_eq!(data.capacity(), n);
        assert_eq!(data.len(), n);
        let pointer = data.as_ptr();
        let boxed = data.into_boxed_slice();
        assert_eq!(boxed.as_ptr(), pointer);
    }
}
