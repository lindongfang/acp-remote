//! Independent public consumer acceptance of the disposable view container, not a node loop.
use acp_remote::domain::{EntityId, native::*, views::*};

const fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
fn target(source: u8, name: &str) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(id(source)), name).unwrap()
}
const fn epoch(runtime: u8, connection: u8, load: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(id(runtime)),
        connection: HarnessConnectionEpoch(id(connection)),
        load: LoadEpoch(id(load)),
    }
}
const fn e() -> ResourceEpoch {
    epoch(1, 2, 3)
}
const fn key(name: &str) -> SourceKey<'_> {
    SourceKey {
        namespace: KeyNamespace::Tool,
        id: name,
    }
}
fn small(bytes: usize, items: usize) -> ViewLimits {
    ViewLimits {
        target_bytes: bytes,
        target_items: items,
        ..ViewLimits::default()
    }
}
fn data(s: &ViewSnapshot) -> Vec<(u64, Vec<u8>)> {
    s.items().map(|i| (i.position, i.bytes.to_vec())).collect()
}
fn usage(v: &ViewWindows, t: &NativeSessionKey, bytes: usize, items: usize) {
    assert_eq!(v.target_usage(t), Some(ViewUsage { bytes, items }));
}

#[test]
fn cl006_051_064_scope_sources_targets_and_each_resource_generation() {
    let mut a = ViewWindows::new(ViewLimits::default()).unwrap();
    let mut b = ViewWindows::new(ViewLimits::default()).unwrap();
    let t = target(1, "same");
    let mut h = a.begin(t.clone(), e()).unwrap();
    let foreign = b.begin(t.clone(), e()).unwrap();
    assert_eq!(h.generation(), foreign.generation());
    assert_eq!(a.insert(&foreign, None, b"x"), Err(ViewError::Stale));
    assert_eq!(a.replace(&foreign, key("x"), b"x"), Err(ViewError::Stale));
    assert_eq!(a.finish(&foreign), Err(ViewError::Stale));
    assert_eq!(
        a.fail(&foreign, ViewFailure::Structure),
        Err(ViewError::Stale)
    );
    assert_eq!(a.release(&foreign), Err(ViewError::Stale));
    assert!(matches!(a.snapshot(&foreign), Err(ViewError::Stale)));
    for other in [target(2, "same"), target(1, "different")] {
        let handle = a.begin(other, e()).unwrap();
        a.insert(&handle, None, b"other").unwrap();
    }
    for resource in [e(), epoch(1, 2, 4), epoch(1, 5, 4), epoch(6, 5, 4)] {
        a.insert(&h, None, b"old").unwrap();
        let snap = a.snapshot(&h).unwrap();
        a.finish(&h).unwrap();
        let next = a.begin(t.clone(), resource).unwrap();
        assert!(next.generation() > h.generation());
        assert_eq!(a.insert(&h, None, b"late"), Err(ViewError::Stale));
        assert_eq!(a.finish(&h), Err(ViewError::Stale));
        assert_eq!(a.release(&h), Err(ViewError::Stale));
        assert_eq!(snap.handle().generation(), h.generation());
        assert_eq!(snap.handle().resource(), h.resource());
        assert_eq!(data(&snap), vec![(1, b"old".to_vec())]);
        assert_eq!(a.snapshot(&next).unwrap().items().len(), 0);
        h = next;
    }
    assert_eq!(a.identity_count(), 3);
}

#[test]
fn cl042_050_052_reading_busy_and_first_failure_stays_sticky() {
    let mut v = ViewWindows::new(ViewLimits::default()).unwrap();
    let t = target(1, "n");
    let h = v.begin(t.clone(), e()).unwrap();
    v.insert(&h, None, b"in flight").unwrap();
    let before = v.usage();
    assert!(matches!(
        v.begin(t.clone(), epoch(9, 9, 9)),
        Err(ViewError::Busy)
    ));
    assert_eq!(v.usage(), before);
    assert_eq!(v.snapshot(&h).unwrap().state(), ViewState::Reading);
    v.fail(&h, ViewFailure::SourceRead).unwrap();
    v.fail(&h, ViewFailure::ConnectionLost).unwrap();
    assert_eq!(v.finish(&h), Err(ViewError::Incomplete));
    let old = v.snapshot(&h).unwrap();
    assert_eq!(old.state(), ViewState::Incomplete(ViewFailure::SourceRead));
    assert!(!old.history_complete());
    let new = v.begin(t, e()).unwrap();
    assert_eq!(v.snapshot(&new).unwrap().state(), ViewState::Reading);
    v.finish(&new).unwrap();
    v.finish(&new).unwrap();
    assert!(v.snapshot(&new).unwrap().history_complete());
    assert!(!old.history_complete());
}

#[test]
fn cl024_042_first_snapshot_preserves_opaque_bytes_and_immutable_older_views() {
    let mut v = ViewWindows::new(ViewLimits::default()).unwrap();
    let h = v.begin(target(1, "n"), e()).unwrap();
    let empty = v.snapshot(&h).unwrap();
    let opaque = b"\0\xff{invalid json}<script>https://untrusted/unknown-extension";
    v.insert(&h, None, opaque).unwrap();
    let first = v.snapshot(&h).unwrap();
    v.insert(&h, None, b"later").unwrap();
    assert_eq!(empty.items().len(), 0);
    assert_eq!(data(&first), vec![(1, opaque.to_vec())]);
    assert_eq!(first.state(), ViewState::Reading);
    assert!(!first.history_complete());
    assert_eq!(v.snapshot(&h).unwrap().items().len(), 2);
    // A lease remains an independently readable old view after its owning registry is dropped.
    drop(v);
    assert_eq!(data(&first), vec![(1, opaque.to_vec())]);
}

#[test]
fn cl012_023_024_051_keys_are_namespaced_updates_keep_position_no_content_dedup() {
    let mut v = ViewWindows::new(ViewLimits::default()).unwrap();
    let h = v.begin(target(1, "n"), e()).unwrap();
    assert_eq!(v.insert(&h, None, b"same"), Ok(1));
    assert_eq!(v.insert(&h, None, b"same"), Ok(2));
    assert_eq!(v.insert(&h, Some(key("k")), b"pending"), Ok(3));
    assert_eq!(v.insert(&h, None, b"after tool"), Ok(4));
    let other = SourceKey {
        namespace: KeyNamespace::AgentMessage,
        id: "k",
    };
    assert_eq!(v.insert(&h, Some(other), b"agent"), Ok(5));
    let old = v.snapshot(&h).unwrap();
    assert_eq!(
        v.insert(&h, Some(key("k")), b"duplicate"),
        Err(ViewError::Conflict)
    );
    assert_eq!(
        v.replace(&h, key("absent"), b"not append"),
        Err(ViewError::MissingItem)
    );
    assert_eq!(v.replace(&h, key("k"), b"full completed object"), Ok(3));
    let new = v.snapshot(&h).unwrap();
    assert_eq!(
        new.items().map(|i| i.position).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert_eq!(data(&old)[2].1, b"pending");
    assert_eq!(data(&new)[2].1, b"full completed object");
    assert_eq!(data(&new)[4].1, b"agent");
    v.finish(&h).unwrap();
    let next = v.begin(target(1, "n"), e()).unwrap();
    assert_eq!(
        v.replace(&next, key("k"), b"old key"),
        Err(ViewError::MissingItem)
    );
}

#[test]
fn cl024_026_utf8_identity_key_limits_and_exact_costs() {
    let mut v = ViewWindows::new(ViewLimits::default()).unwrap();
    assert_eq!(
        v.usage(),
        ViewUsage {
            bytes: 8192,
            items: 0
        }
    );
    let native = "界".repeat(1365) + "x"; // 4096 bytes, not 4096 characters.
    let t = target(1, &native);
    let h = v.begin(t.clone(), e()).unwrap();
    usage(&v, &t, 1024 + 4096 + 1024, 0);
    let k = "é".repeat(2048);
    v.insert(&h, Some(key(&k)), b"abc").unwrap();
    usage(&v, &t, 1024 + 4096 + 1024 + 512 + 4096 + 3, 1);
    let snap = v.snapshot(&h).unwrap();
    usage(&v, &t, 1024 + 4096 + 1024 + 512 + 4096 + 3 + 256 + 64, 2);
    assert_eq!(
        snap.items().next().unwrap().key.unwrap().id.as_bytes(),
        k.as_bytes()
    );
    let before = v.usage();
    let too_long = k + "x";
    assert_eq!(
        v.insert(&h, Some(key(&too_long)), b""),
        Err(ViewError::InvalidKey)
    );
    assert_eq!(
        v.replace(&h, key(&too_long), b""),
        Err(ViewError::InvalidKey)
    );
    assert_eq!(v.usage(), before);
    assert!(NativeSessionKey::from_config(Some(id(1)), &(native + "x")).is_err());
    assert_eq!(v.usage().bytes, 8192 + v.target_usage(&t).unwrap().bytes);
}

#[test]
fn cl026_default_ceilings_and_invalid_limits_do_not_allocate() {
    assert_eq!(
        ViewLimits::default(),
        ViewLimits {
            target_bytes: 16 * 1024 * 1024,
            node_bytes: 128 * 1024 * 1024,
            target_items: 4096,
            node_items: 32768
        }
    );
    let d = ViewLimits::default();
    for l in [
        ViewLimits {
            target_bytes: usize::MAX,
            ..d
        },
        ViewLimits {
            node_bytes: usize::MAX,
            ..d
        },
        ViewLimits {
            target_items: usize::MAX,
            ..d
        },
        ViewLimits {
            node_items: usize::MAX,
            ..d
        },
        ViewLimits {
            node_bytes: 8191,
            ..d
        },
        ViewLimits {
            target_items: 0,
            ..d
        },
        ViewLimits { node_items: 0, ..d },
    ] {
        assert!(matches!(ViewWindows::new(l), Err(ViewError::InvalidLimits)));
    }
    let mut v = ViewWindows::new(ViewLimits {
        node_bytes: 8192,
        ..d
    })
    .unwrap();
    assert!(matches!(
        v.begin(target(1, "n"), e()),
        Err(ViewError::Capacity)
    ));
    assert_eq!(
        v.usage(),
        ViewUsage {
            bytes: 8192,
            items: 0
        }
    );
    assert_eq!(v.identity_count(), 0);
}

#[test]
fn cl026_042_empty_snapshot_exact_byte_boundary_and_no_leak() {
    for deficit in [0, 1] {
        let mut v = ViewWindows::new(small(2049 + 256 - deficit, 10)).unwrap();
        let t = target(1, "n");
        let h = v.begin(t.clone(), e()).unwrap();
        let baseline = v.usage();
        if deficit == 0 {
            let s = v.snapshot(&h).unwrap();
            usage(&v, &t, 2305, 0);
            assert!(matches!(v.snapshot(&h), Err(ViewError::Capacity)));
            usage(&v, &t, 2305, 0);
            drop(s);
        } else {
            for _ in 0..20 {
                assert!(matches!(v.snapshot(&h), Err(ViewError::Capacity)));
            }
        }
        assert_eq!(v.usage(), baseline);
        v.release(&h).unwrap();
        assert_eq!(v.usage().bytes, 8192);
        assert_eq!(v.identity_count(), 0);
    }
}

#[test]
fn cl026_049_snapshot_bytes_and_item_refs_both_precharged_at_boundary() {
    // Two empty entries cost 1024; one snapshot costs 384 and two references.
    for (bytes, items, success) in [
        (2049 + 1024 + 384, 4, true),
        (2049 + 1024 + 383, 4, false),
        (10000, 3, false),
    ] {
        let mut v = ViewWindows::new(small(bytes, items)).unwrap();
        let h = v.begin(target(1, "n"), e()).unwrap();
        v.insert(&h, None, b"").unwrap();
        v.insert(&h, None, b"").unwrap();
        let before = v.usage();
        if success {
            let snap = v.snapshot(&h).unwrap();
            assert_eq!(
                v.usage(),
                ViewUsage {
                    bytes: before.bytes + 384,
                    items: 4
                }
            );
            assert!(matches!(v.snapshot(&h), Err(ViewError::Capacity)));
            drop(snap);
            assert!(v.snapshot(&h).is_ok());
        } else {
            for _ in 0..10 {
                assert!(matches!(v.snapshot(&h), Err(ViewError::Capacity)));
            }
        }
        assert_eq!(v.usage(), before);
    }
}

#[test]
fn cl026_049_node_byte_boundary_is_shared_and_never_clears_other_target() {
    for deficit in [0, 1] {
        let mut v = ViewWindows::new(ViewLimits {
            node_bytes: 8192 + 2 * 2049 + 512 - deficit,
            ..small(10000, 20)
        })
        .unwrap();
        let a = v.begin(target(1, "n"), e()).unwrap();
        let b = v.begin(target(2, "n"), e()).unwrap();
        let result = v.insert(&a, None, b"");
        assert_eq!(
            result,
            if deficit == 0 {
                Ok(1)
            } else {
                Err(ViewError::Capacity)
            }
        );
        assert_eq!(
            v.target_usage(&target(2, "n")).unwrap(),
            ViewUsage {
                bytes: 2049,
                items: 0
            }
        );
        v.release(&b).unwrap();
        assert!(v.snapshot(&a).is_ok());
    }
}

#[test]
fn cl026_049_node_items_include_references_across_targets() {
    let mut v = ViewWindows::new(ViewLimits {
        node_items: 3,
        ..ViewLimits::default()
    })
    .unwrap();
    let a = v.begin(target(1, "n"), e()).unwrap();
    let b = v.begin(target(2, "n"), e()).unwrap();
    v.insert(&a, None, b"A").unwrap();
    v.insert(&b, None, b"B").unwrap();
    let sa = v.snapshot(&a).unwrap(); // third global count
    let before = v.usage();
    assert!(matches!(v.snapshot(&b), Err(ViewError::Capacity)));
    assert_eq!(v.usage(), before);
    drop(sa);
    assert_eq!(data(&v.snapshot(&b).unwrap()), vec![(1, b"B".to_vec())]);
}

#[test]
fn cl049_050_long_stream_has_bounded_fifo_watermark_and_finished_is_not_complete() {
    // Byte pressure retains four empty objects; snapshot needs another 512 bytes.
    let mut v = ViewWindows::new(small(2049 + 4 * 512 + 512, 100)).unwrap();
    let h = v.begin(target(1, "n"), e()).unwrap();
    let other = v.begin(target(2, "n"), e()).unwrap();
    v.insert(&other, None, b"untouched").unwrap();
    for n in 1..=10000 {
        assert_eq!(v.insert(&h, None, b""), Ok(n));
    }
    // Five objects fit; free space is zero. A snapshot cannot invent a free reference buffer.
    assert!(matches!(v.snapshot(&h), Err(ViewError::Capacity)));
    // Create pressure with a held lease from a differently sized fresh stream below.
    v.finish(&h).unwrap();
    v.release(&h).unwrap();
    assert_eq!(
        data(&v.snapshot(&other).unwrap()),
        vec![(1, b"untouched".to_vec())]
    );
    let mut v = ViewWindows::new(small(2049 + 4 * 600 + 512, 100)).unwrap();
    let h = v.begin(target(1, "n"), e()).unwrap();
    for n in 1..=10000 {
        assert_eq!(v.insert(&h, None, &[0; 88]), Ok(n));
    }
    v.finish(&h).unwrap();
    let s = v.snapshot(&h).unwrap();
    assert_eq!(s.items().len(), 4);
    assert_eq!(s.first_position(), Some(9997));
    assert_eq!(s.evicted_through(), Some(9996));
    assert_eq!(s.evicted_count(), 9996);
    assert_eq!(s.state(), ViewState::Finished);
    assert!(!s.history_complete());
}

#[test]
fn cl023_050_evicted_key_is_missing_and_oversized_insert_is_sticky() {
    let mut v = ViewWindows::new(small(3000, 20)).unwrap();
    let h = v.begin(target(1, "n"), e()).unwrap();
    v.insert(&h, Some(key("old")), b"a").unwrap();
    v.insert(&h, Some(key("new")), b"b").unwrap(); // evicts old
    assert_eq!(
        v.replace(&h, key("old"), b"late patch"),
        Err(ViewError::MissingItem)
    );
    assert_eq!(data(&v.snapshot(&h).unwrap()), vec![(2, b"b".to_vec())]);
    assert_eq!(v.insert(&h, None, &[0; 3000]), Err(ViewError::Capacity));
    assert_eq!(v.finish(&h), Err(ViewError::Incomplete));
    assert_eq!(
        v.snapshot(&h).unwrap().state(),
        ViewState::Incomplete(ViewFailure::Capacity)
    );
}

#[test]
fn cl023_026_replace_peak_fails_atomically_with_and_without_old_lease() {
    for leased in [false, true] {
        // Two entries (516 each), optional old snapshot (384), then candidate is one byte short.
        let max = 2049 + 2 * 516 + usize::from(leased) * 384 + 516 - 1;
        let mut v = ViewWindows::new(small(max, 20)).unwrap();
        let h = v.begin(target(1, "n"), e()).unwrap();
        v.insert(&h, Some(key("k")), b"old").unwrap();
        v.insert(&h, Some(key("z")), b"end").unwrap();
        let old = leased.then(|| v.snapshot(&h).unwrap());
        let before = v.usage();
        assert_eq!(v.replace(&h, key("k"), b"new"), Err(ViewError::Capacity));
        assert_eq!(v.usage(), before);
        drop(old);
        let s = v.snapshot(&h).unwrap();
        assert_eq!(data(&s), vec![(1, b"old".to_vec()), (2, b"end".to_vec())]);
        assert_eq!(s.evicted_count(), 0);
        assert_eq!(s.state(), ViewState::Incomplete(ViewFailure::Capacity));
    }
}

#[test]
fn cl023_026_replace_exact_peak_and_old_data_release_only_after_last_snapshot() {
    let mut v = ViewWindows::new(small(2049 + 516 + 320 + 516, 20)).unwrap();
    let t = target(1, "n");
    let h = v.begin(t.clone(), e()).unwrap();
    v.insert(&h, Some(key("k")), b"old").unwrap();
    let old = v.snapshot(&h).unwrap();
    assert_eq!(v.replace(&h, key("k"), b"new"), Ok(1));
    usage(&v, &t, 2049 + 2 * 516 + 320, 3);
    assert_eq!(data(&old), vec![(1, b"old".to_vec())]);
    drop(old);
    usage(&v, &t, 2049 + 516, 1);
    assert_eq!(data(&v.snapshot(&h).unwrap()), vec![(1, b"new".to_vec())]);
}

#[test]
fn cl049_051_064_cross_generation_old_leases_share_target_byte_and_item_limits() {
    for resource in [e(), epoch(1, 2, 4), epoch(1, 5, 3), epoch(6, 2, 3)] {
        for byte_limit in [true, false] {
            let limits = if byte_limit {
                small(2049 + 513 + 320, 10)
            } else {
                small(10000, 2)
            };
            let mut v = ViewWindows::new(limits).unwrap();
            let t = target(1, "n");
            let old = v.begin(t.clone(), e()).unwrap();
            v.insert(&old, None, b"x").unwrap();
            let lease = v.snapshot(&old).unwrap();
            v.release(&old).unwrap();
            let new = v.begin(t.clone(), resource).unwrap();
            let before = v.usage();
            assert_eq!(v.insert(&new, None, b"y"), Err(ViewError::Capacity));
            assert_eq!(v.usage(), before);
            assert_eq!(v.identity_count(), 1);
            drop(lease);
            usage(&v, &t, 2049, 0);
            assert_eq!(v.insert(&new, None, b"y"), Ok(1));
            assert_eq!(v.finish(&new), Err(ViewError::Incomplete));
        }
    }
}

#[test]
fn cl026_049_lease_prevents_eviction_from_minting_room_and_drops_exactly() {
    let mut v = ViewWindows::new(small(2049 + 513 + 2 * 320, 20)).unwrap();
    let t = target(1, "n");
    let h = v.begin(t.clone(), e()).unwrap();
    v.insert(&h, None, b"x").unwrap();
    let first = v.snapshot(&h).unwrap();
    let second = v.snapshot(&h).unwrap();
    usage(&v, &t, 2049 + 513 + 640, 3);
    // Eviction removes the current reference but both leases still own the immutable object.
    assert_eq!(v.insert(&h, None, &[0; 128]), Err(ViewError::Capacity));
    usage(&v, &t, 2049 + 513 + 640, 3);
    assert_eq!(data(&first), vec![(1, b"x".to_vec())]);
    drop(first);
    usage(&v, &t, 2049 + 513 + 320, 2);
    drop(second);
    usage(&v, &t, 2049, 0);
    v.release(&h).unwrap();
    assert_eq!(
        v.usage(),
        ViewUsage {
            bytes: 8192,
            items: 0
        }
    );
}

#[test]
fn cl051_063_refresh_invalidates_old_handle_and_first_begin_failure_reclaims_identity() {
    // Old leases and the new window together exactly fill this target budget.
    let mut v = ViewWindows::new(small(2049 + 512 + 3 * 320, 20)).unwrap();
    let t = target(1, "n");
    let h = v.begin(t.clone(), e()).unwrap();
    v.insert(&h, None, b"").unwrap();
    let first = v.snapshot(&h).unwrap();
    let second = v.snapshot(&h).unwrap();
    let third = v.snapshot(&h).unwrap();
    v.finish(&h).unwrap();
    // Same-size window fits after removing its predecessor. This is a successful refresh,
    // not evidence of the unreachable public-API ended-begin reservation-failure branch.
    let next = v.begin(t.clone(), e()).unwrap();
    assert!(matches!(v.snapshot(&next), Err(ViewError::Capacity)));
    assert_eq!(v.finish(&h), Err(ViewError::Stale));
    assert_eq!(first.handle().generation(), h.generation());
    assert_eq!(v.snapshot(&h).err(), Some(ViewError::Stale));
    drop((first, second, third));
    v.release(&next).unwrap();
    assert_eq!(v.identity_count(), 0);
    // A first begin can fail after reserving an identity but cannot expose any current handle.
    let mut tight = ViewWindows::new(small(1025, 10)).unwrap();
    assert!(matches!(tight.begin(t, e()), Err(ViewError::Capacity)));
    tight.collect_released();
    assert_eq!(tight.identity_count(), 0);
}

#[test]
fn cl049_064_sixty_four_identities_wait_for_last_lease_and_reclaim_cleanly() {
    let mut v = ViewWindows::new(ViewLimits::default()).unwrap();
    let mut leases = Vec::new();
    for n in 0..64 {
        let h = v.begin(target(n, "n"), e()).unwrap();
        leases.push((v.snapshot(&h).unwrap(), v.snapshot(&h).unwrap()));
        v.release(&h).unwrap();
    }
    assert_eq!(v.identity_count(), 64);
    assert!(matches!(
        v.begin(target(65, "n"), e()),
        Err(ViewError::Capacity)
    ));
    let (a, b) = leases.pop().unwrap();
    drop(a);
    v.collect_released();
    assert_eq!(v.identity_count(), 64);
    assert!(matches!(
        v.begin(target(65, "n"), e()),
        Err(ViewError::Capacity)
    ));
    drop(b);
    v.collect_released();
    assert_eq!(v.identity_count(), 63);
    let h = v.begin(target(65, "n"), e()).unwrap();
    assert_eq!(v.identity_count(), 64);
    v.release(&h).unwrap();
    drop(leases);
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
fn cl026_049_production_item_limits_include_refs_and_cross_thread_drop_refunds() {
    let mut views = ViewWindows::new(ViewLimits::default()).unwrap();
    let mut leases = Vec::new();
    let mut handles = Vec::new();
    for source in 0..8 {
        let handle = views.begin(target(source, "n"), e()).unwrap();
        for position in 1..=2048 {
            assert_eq!(views.insert(&handle, None, b""), Ok(position));
        }
        leases.push(views.snapshot(&handle).unwrap());
        assert_eq!(
            views.target_usage(&target(source, "n")).unwrap().items,
            4096
        );
        let before = views.usage();
        assert!(matches!(views.snapshot(&handle), Err(ViewError::Capacity)));
        assert_eq!(views.usage(), before);
        handles.push(handle);
    }
    assert_eq!(views.usage().items, 32768);
    let extra = views.begin(target(9, "n"), e()).unwrap();
    assert_eq!(views.insert(&extra, None, b""), Err(ViewError::Capacity));
    let last = leases.pop().unwrap();
    std::thread::spawn(move || drop(last)).join().unwrap();
    assert_eq!(views.usage().items, 30720);
    assert_eq!(views.insert(&extra, None, b""), Ok(1));
    drop(leases);
    for handle in handles {
        views.release(&handle).unwrap();
    }
    views.release(&extra).unwrap();
    assert_eq!(
        views.usage(),
        ViewUsage {
            bytes: 8192,
            items: 0
        }
    );
}
