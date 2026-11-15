use super::*;
use crate::domain::{
    EntityId,
    native::{HarnessConnectionEpoch, LoadEpoch, RuntimeEpoch},
};
fn key(n: u8) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(EntityId::from_bytes([n; 16])), "native").unwrap()
}
fn epoch(n: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([1; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([2; 16])),
        load: LoadEpoch(EntityId::from_bytes([n; 16])),
    }
}
fn limits(bytes: usize, items: usize) -> ViewLimits {
    ViewLimits {
        target_bytes: bytes,
        node_bytes: 128 * 1024 * 1024,
        target_items: items,
        node_items: 32768,
    }
}
fn source(id: &str) -> SourceKey<'_> {
    SourceKey {
        namespace: KeyNamespace::Tool,
        id,
    }
}

#[test]
fn first_read_is_immediate_opaque_and_replacement_keeps_source_position() {
    let mut views = ViewWindows::new(ViewLimits::default()).unwrap();
    let handle = views.begin(key(1), epoch(1)).unwrap();
    let first = views
        .insert(&handle, Some(source("tool")), b"\0<script>unknown")
        .unwrap();
    let second = views.insert(&handle, None, b"later").unwrap();
    let old = views.snapshot(&handle).unwrap();
    assert_eq!(old.state(), ViewState::Reading);
    assert!(!old.history_complete());
    assert_eq!(
        views.replace(&handle, source("tool"), b"finished"),
        Ok(first)
    );
    views.finish(&handle).unwrap();
    let new = views.snapshot(&handle).unwrap();
    assert_eq!(
        new.items().map(|item| item.position).collect::<Vec<_>>(),
        vec![first, second]
    );
    assert_eq!(old.items().next().unwrap().bytes, b"\0<script>unknown");
    assert_eq!(new.items().next().unwrap().bytes, b"finished");
    assert!(new.history_complete());
    assert_eq!(
        views.insert(&handle, Some(source("tool")), b"duplicate"),
        Err(ViewError::Conflict)
    );
    assert_eq!(
        views.replace(&handle, source("absent"), b"no upsert"),
        Err(ViewError::MissingItem)
    );
}

#[test]
fn scope_generation_and_sticky_failure_do_not_cross_reads() {
    let mut a = ViewWindows::new(ViewLimits::default()).unwrap();
    let mut b = ViewWindows::new(ViewLimits::default()).unwrap();
    let old = a.begin(key(1), epoch(1)).unwrap();
    let foreign = b.begin(key(1), epoch(1)).unwrap();
    assert_eq!(a.insert(&foreign, None, b"foreign"), Err(ViewError::Stale));
    assert!(matches!(a.begin(key(1), epoch(1)), Err(ViewError::Busy)));
    a.fail(&old, ViewFailure::ConnectionLost).unwrap();
    a.fail(&old, ViewFailure::SourceRead).unwrap();
    assert_eq!(a.finish(&old), Err(ViewError::Incomplete));
    assert_eq!(
        a.snapshot(&old).unwrap().state(),
        ViewState::Incomplete(ViewFailure::ConnectionLost)
    );
    let current = a.begin(key(1), epoch(2)).unwrap();
    assert_ne!(old.generation(), current.generation());
    assert_eq!(a.finish(&old), Err(ViewError::Stale));
    assert_eq!(a.release(&old), Err(ViewError::Stale));
    assert_eq!(a.snapshot(&current).unwrap().state(), ViewState::Reading);
}

#[test]
fn exact_charge_includes_empty_snapshots_and_refs_then_drops_cleanly() {
    let mut views = ViewWindows::new(ViewLimits::default()).unwrap();
    assert_eq!(
        views.usage(),
        ViewUsage {
            bytes: ROOT_BYTES,
            items: 0
        }
    );
    let target = key(1);
    let handle = views.begin(target.clone(), epoch(1)).unwrap();
    let base = IDENTITY_BYTES + target.native_id().len() + WINDOW_BYTES;
    assert_eq!(
        views.target_usage(&target),
        Some(ViewUsage {
            bytes: base,
            items: 0
        })
    );
    let empty = views.snapshot(&handle).unwrap();
    assert_eq!(
        views.target_usage(&target).unwrap().bytes,
        base + SNAPSHOT_BYTES
    );
    drop(empty);
    views.insert(&handle, Some(source("k")), b"data").unwrap();
    let snapshot = views.snapshot(&handle).unwrap();
    let charged = base + ITEM_BYTES + 1 + 4 + SNAPSHOT_BYTES + REFERENCE_BYTES;
    assert_eq!(
        views.target_usage(&target),
        Some(ViewUsage {
            bytes: charged,
            items: 2
        })
    );
    views.release(&handle).unwrap();
    assert_eq!(
        views.target_usage(&target).unwrap().bytes,
        charged - WINDOW_BYTES
    );
    assert_eq!(views.identity_count(), 1);
    drop(snapshot);
    views.collect_released();
    assert_eq!(views.identity_count(), 0);
    assert_eq!(
        views.usage(),
        ViewUsage {
            bytes: ROOT_BYTES,
            items: 0
        }
    );
}

#[test]
fn stream_evicts_only_local_prefix_and_does_not_claim_full_history() {
    let mut views = ViewWindows::new(limits(16 * 1024, 4)).unwrap();
    let first = views.begin(key(1), epoch(1)).unwrap();
    let other = views.begin(key(2), epoch(1)).unwrap();
    views.insert(&other, None, b"independent").unwrap();
    for _ in 0..10000 {
        views.insert(&first, None, b"data").unwrap();
    }
    // Four current objects consume the four target item slots; snapshots need their own slots.
    assert!(matches!(views.snapshot(&first), Err(ViewError::Capacity)));
    assert_eq!(
        views
            .snapshot(&other)
            .unwrap()
            .items()
            .next()
            .unwrap()
            .bytes,
        b"independent"
    );
    views.finish(&first).unwrap();
    let slot = views.slot_mut(&first).unwrap();
    let window = slot.window.as_ref().unwrap();
    assert_eq!(window.items.len(), 4);
    assert_eq!(window.evicted_count, 9996);
    assert_eq!(window.evicted_through, Some(9996));
    assert_eq!(window.items.front().unwrap().position, 9997);
    assert_eq!(window.status.state(), ViewState::Finished);
}

#[test]
fn retained_leases_across_resource_change_cannot_mint_new_target_room() {
    let base = IDENTITY_BYTES + 6 + WINDOW_BYTES;
    let max = base + ITEM_BYTES + 4 + SNAPSHOT_BYTES + REFERENCE_BYTES;
    let mut views = ViewWindows::new(limits(max, 10)).unwrap();
    let old = views.begin(key(1), epoch(1)).unwrap();
    views.insert(&old, None, b"data").unwrap();
    let lease = views.snapshot(&old).unwrap();
    views.release(&old).unwrap();
    let new = views.begin(key(1), epoch(2)).unwrap();
    assert_eq!(views.insert(&new, None, b"again"), Err(ViewError::Capacity));
    // Old lease plus new window exactly fill the budget, so even an empty lease is refused.
    assert!(matches!(views.snapshot(&new), Err(ViewError::Capacity)));
    assert_eq!(
        views
            .slot_mut(&new)
            .unwrap()
            .window
            .as_ref()
            .unwrap()
            .status
            .state(),
        ViewState::Incomplete(ViewFailure::Capacity)
    );
    drop(lease);
    assert!(views.insert(&new, None, b"again").is_ok());
    assert_eq!(views.finish(&new), Err(ViewError::Incomplete));
}

#[test]
fn replace_reserves_both_versions_even_without_snapshot_and_failure_preserves_old() {
    let base = IDENTITY_BYTES + 6 + WINDOW_BYTES;
    let mut views = ViewWindows::new(limits(base + 2 * (ITEM_BYTES + 1 + 3) - 1, 10)).unwrap();
    let handle = views.begin(key(1), epoch(1)).unwrap();
    views.insert(&handle, Some(source("k")), b"old").unwrap();
    let before = views.usage();
    assert_eq!(
        views.replace(&handle, source("k"), b"new"),
        Err(ViewError::Capacity)
    );
    assert_eq!(views.usage(), before);
    let snap = views.snapshot(&handle).unwrap();
    assert_eq!(snap.items().next().unwrap().bytes, b"old");
    assert_eq!(snap.state(), ViewState::Incomplete(ViewFailure::Capacity));
}

#[test]
fn sixty_four_debted_identities_remain_until_their_last_lease_drops() {
    let mut views = ViewWindows::new(ViewLimits::default()).unwrap();
    let mut leases = Vec::new();
    for n in 0..64 {
        let h = views.begin(key(n), epoch(1)).unwrap();
        leases.push(views.snapshot(&h).unwrap());
        views.release(&h).unwrap();
    }
    assert_eq!(views.identity_count(), 64);
    assert!(matches!(
        views.begin(key(65), epoch(1)),
        Err(ViewError::Capacity)
    ));
    drop(leases.pop());
    assert!(views.begin(key(65), epoch(1)).is_ok());
    assert_eq!(views.identity_count(), 64);
}

#[test]
fn snapshot_failure_preserves_ledger_and_window_while_empty_leases_are_not_free() {
    let base = IDENTITY_BYTES + 6 + WINDOW_BYTES;
    let mut views = ViewWindows::new(limits(base + SNAPSHOT_BYTES, 10)).unwrap();
    let h = views.begin(key(1), epoch(1)).unwrap();
    let lease = views.snapshot(&h).unwrap();
    let before = views.usage();
    assert!(matches!(views.snapshot(&h), Err(ViewError::Capacity)));
    assert_eq!(views.usage(), before);
    assert_eq!(lease.state(), ViewState::Reading);
    drop(lease);
    assert!(views.snapshot(&h).is_ok());
}

#[test]
fn checked_extremes_fail_without_allocating_or_reusing_ids() {
    assert_eq!(item_cost(None, usize::MAX), Err(ViewError::Capacity));
    let mut views = ViewWindows::new(ViewLimits::default()).unwrap();
    views.next_generation = u64::MAX;
    assert!(matches!(
        views.begin(key(1), epoch(1)),
        Err(ViewError::SequenceExhausted)
    ));
    assert_eq!(views.identity_count(), 0);
    views.next_generation = 0;
    let h = views.begin(key(1), epoch(1)).unwrap();
    views
        .slot_mut(&h)
        .unwrap()
        .window
        .as_mut()
        .unwrap()
        .next_position = u64::MAX;
    assert_eq!(
        views.insert(&h, None, b"overflow"),
        Err(ViewError::SequenceExhausted)
    );
    assert_eq!(
        views.snapshot(&h).unwrap().state(),
        ViewState::Incomplete(ViewFailure::SequenceExhausted)
    );
    let ledger = Ledger {
        usage: ViewUsage {
            bytes: usize::MAX,
            items: 0,
        },
        broken: false,
    };
    assert_eq!(
        addition(&ledger, 1, 0, usize::MAX, usize::MAX),
        Err(ViewError::Capacity)
    );
}
