#[path = "acceptance/view_budget_support.rs"]
mod support;
use acp_remote::domain::{native::*, views::*};
use support::*;

#[test]
fn bp_a_lease_is_single_item_with_exact_shared_reference_charge() {
    let mut v = views(100_000, 100);
    let t = target(1);
    let h = v.begin(t, epoch(1)).unwrap();
    for n in 0..10 {
        v.insert(&h, Some(key(&format!("k{n}"))), b"bytes").unwrap();
    }
    let base = v.usage();
    let first_lease = v.item_lease(&h, key("k4")).unwrap();
    let second_lease = v.item_lease(&h, key("k4")).unwrap();
    assert_eq!(
        v.usage(),
        ViewUsage {
            bytes: base.bytes + 640,
            items: base.items + 2
        }
    );
    assert_eq!(first_lease.item().position, 5);
    assert_eq!(first_lease.item().bytes, b"bytes");
    assert_eq!(
        v.item_lease(&h, key("missing")).err(),
        Some(ViewError::MissingItem)
    );
    let too_long = "x".repeat(4097);
    assert_eq!(
        v.item_lease(&h, key(&too_long)).err(),
        Some(ViewError::InvalidKey)
    );
    drop(first_lease);
    assert_eq!(v.usage().bytes, base.bytes + 320);
    drop(second_lease);
    assert_eq!(v.usage(), base);
}

#[test]
fn bp_a_lease_bytes_and_items_boundaries_are_shared_with_snapshots() {
    for (cap, items, ok) in [
        (2049 + 514 + 320, 2, true),
        (2049 + 514 + 319, 2, false),
        (10000, 1, false),
    ] {
        let mut v = views(cap, items);
        let h = v.begin(target(1), epoch(1)).unwrap();
        v.insert(&h, Some(key("k")), b"x").unwrap();
        let base = v.usage();
        let result = v.item_lease(&h, key("k"));
        assert_eq!(result.is_ok(), ok);
        drop(result);
        assert_eq!(v.usage(), base);
    }
}

#[test]
fn bp_b_workspace_explicit_capacity_zero_and_standard_mapping() {
    for n in [0, 1, 7680] {
        let mut v = views(100_000, 100);
        let t = target(1);
        let h = v.begin(t.clone(), epoch(1)).unwrap();
        let mut w = v.stage_workspace(&h, n).unwrap();
        assert_eq!(w.scratch().len(), n);
        w.scratch().fill(0xaa);
        assert_usage(&v, &t, 2049 + 512 + n, 0);
        drop(w);
        assert_usage(&v, &t, 2049, 0);
        assert_eq!(v.window_status(&h).unwrap().state, ViewState::Reading);
        let mut exact = views(2049 + 512 + n, 100);
        let h = exact.begin(target(1), epoch(1)).unwrap();
        drop(exact.stage_workspace(&h, n).unwrap());
        let mut short = views(2049 + 512 + n - 1, 100);
        let h = short.begin(target(1), epoch(1)).unwrap();
        assert!(matches!(
            short.stage_workspace(&h, n),
            Err(ViewError::Capacity)
        ));
        assert_eq!(
            short.window_status(&h).unwrap().state,
            ViewState::Incomplete(ViewFailure::Capacity)
        );
    }
    let mut v = views(100_000, 100);
    let h = v.begin(target(1), epoch(1)).unwrap();
    assert!(matches!(
        v.stage_workspace(&h, usize::MAX),
        Err(ViewError::Capacity)
    ));
    assert_eq!(v.usage().bytes, 8192 + 2049);
}

#[test]
fn bp_b_workspace_keeps_old_window_charge_after_release_until_last_drop() {
    let mut v = views(100_000, 100);
    let t = target(1);
    let old = v.begin(t.clone(), epoch(1)).unwrap();
    let w1 = v.stage_workspace(&old, 7).unwrap();
    let w2 = v.stage_workspace(&old, 11).unwrap();
    v.release(&old).unwrap();
    assert_usage(&v, &t, 1025 + 1024 + 519 + 523, 0);
    assert_eq!(v.identity_count(), 1);
    let new = v.begin(t.clone(), epoch(2)).unwrap();
    assert_usage(&v, &t, 1025 + 2 * 1024 + 519 + 523, 0);
    drop(w1);
    assert_usage(&v, &t, 1025 + 2 * 1024 + 523, 0);
    drop(w2);
    assert_usage(&v, &t, 2049, 0);
    assert_eq!(v.window_status(&new).unwrap().state, ViewState::Reading);
}

#[test]
fn bp_c_exact_writer_all_or_zero_poison_short_write_and_positions() {
    let mut v = views(100_000, 100);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let start = v.window_status(&h).unwrap();
    let w = v.stage_workspace(&h, 0).unwrap();
    let mut stage = v.stage_insert(&h, w, None, 3).unwrap();
    {
        let mut p = stage.parts();
        assert_eq!(p.writer.remaining(), 3);
        p.writer.write_all(b"ab").unwrap();
        assert_eq!(p.writer.write_all(b"cd"), Err(ViewError::WriteTooLong));
        assert_eq!(p.writer.written(), 2);
        assert_eq!(p.writer.remaining(), 1);
        assert_eq!(p.writer.write_all(b"c"), Err(ViewError::PoisonedStage));
    }
    assert_eq!(v.publish(&h, stage), Err(ViewError::PoisonedStage));
    assert_eq!(
        v.window_status(&h).unwrap().next_position,
        start.next_position
    );
    assert_eq!(v.window_status(&h).unwrap().retained_items, 0);
    let w = v.stage_workspace(&h, 0).unwrap();
    let mut short = v.stage_insert(&h, w, None, 2).unwrap();
    write(&mut short, b"a");
    assert_eq!(v.publish(&h, short), Err(ViewError::ShortWrite));
    let w = v.stage_workspace(&h, 0).unwrap();
    let mut full = v.stage_insert(&h, w, None, 3).unwrap();
    {
        let mut p = full.parts();
        p.writer.write_all(b"").unwrap();
        p.writer.write_all(b"\0\xffx").unwrap();
        assert_eq!(p.writer.written(), 3);
        assert_eq!(p.writer.remaining(), 0);
    }
    assert_eq!(v.window_status(&h).unwrap().retained_items, 0);
    let fact = v.publish(&h, full).unwrap();
    assert_eq!(fact.position, 1);
    assert!(!fact.replaced);
    assert_eq!(fact.status, v.window_status(&h).unwrap());
    assert_eq!(fact.status.next_position, Some(2));
    assert_eq!(v.finish(&h), Err(ViewError::Incomplete));
}

#[test]
fn bp_c_zero_payload_and_invalid_key_or_length_fail_before_eviction() {
    let mut v = views(100_000, 100);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let w = v.stage_workspace(&h, 0).unwrap();
    let zero = v.stage_insert(&h, w, None, 0).unwrap();
    assert_eq!(v.publish(&h, zero).unwrap().position, 1);
    for bad in [false, true] {
        let w = v.stage_workspace(&h, 0).unwrap();
        let huge = "x".repeat(4097);
        let result = if bad {
            v.stage_insert(&h, w, Some(key(&huge)), 1)
        } else {
            v.stage_insert(&h, w, None, usize::MAX)
        };
        assert_eq!(
            result.err(),
            Some(if bad {
                ViewError::InvalidKey
            } else {
                ViewError::Capacity
            })
        );
        assert_eq!(v.window_status(&h).unwrap().evicted_count, 0);
        assert_eq!(v.window_status(&h).unwrap().next_position, Some(2));
    }
}

#[test]
fn bp_d_replace_peak_exact_byte_and_item_limits_preserve_old_on_failure() {
    // B2049 + old516 + lease320 + scratch512 + candidate516 =3913.
    for (cap, items, ok) in [(3913, 3, true), (3912, 3, false), (10000, 2, false)] {
        let mut v = views(cap, items);
        let t = target(1);
        let h = v.begin(t.clone(), epoch(1)).unwrap();
        v.insert(&h, Some(key("k")), b"old").unwrap();
        let lease = v.item_lease(&h, key("k")).unwrap();
        let w = v.stage_workspace(&h, 0).unwrap();
        let result = v.stage_replace(&h, w, lease, 3);
        if ok {
            let mut stage = result.unwrap();
            assert_usage(&v, &t, 3913, 3);
            assert_eq!(stage.parts().old.unwrap().bytes, b"old");
            write(&mut stage, b"new");
            let fact = v.publish(&h, stage).unwrap();
            assert!(fact.replaced);
            assert_eq!(fact.position, 1);
            assert_eq!(fact.status.next_position, Some(2));
            assert_usage(&v, &t, 2049 + 516, 1);
        } else {
            assert!(matches!(result, Err(ViewError::Capacity)));
            assert_usage(&v, &t, 2049 + 516, 1);
            assert_eq!(v.window_status(&h).unwrap().evicted_count, 0);
        }
    }
}

#[test]
fn bp_d_consumed_lease_not_double_charged_snapshot_old_bytes_live_to_last_drop() {
    let mut v = views(100_000, 100);
    let t = target(1);
    let h = v.begin(t.clone(), epoch(1)).unwrap();
    v.insert(&h, Some(key("k")), b"old").unwrap();
    let snap = v.snapshot(&h).unwrap();
    let lease = v.item_lease(&h, key("k")).unwrap();
    let extra = v.item_lease(&h, key("k")).unwrap();
    let w = v.stage_workspace(&h, 7680).unwrap();
    let mut stage = v.stage_replace(&h, w, lease, 3).unwrap();
    assert_usage(&v, &t, 2049 + 516 + 320 + 640 + 8192 + 516, 5);
    write(&mut stage, b"new");
    v.publish(&h, stage).unwrap();
    assert_usage(&v, &t, 2049 + 2 * 516 + 320 + 320, 4);
    assert_eq!(snap.items().next().unwrap().bytes, b"old");
    assert_eq!(extra.item().bytes, b"old");
    drop(extra);
    assert_usage(&v, &t, 2049 + 2 * 516 + 320, 3);
    drop(snap);
    assert_usage(&v, &t, 2049 + 516, 1);
}

#[test]
fn bp_e_two_candidates_private_item_cas_and_pregrant_conflict() {
    let mut v = views(100_000, 100);
    let h = v.begin(target(1), epoch(1)).unwrap();
    v.insert(&h, Some(key("k")), b"old").unwrap();
    let l1 = v.item_lease(&h, key("k")).unwrap();
    let l2 = v.item_lease(&h, key("k")).unwrap();
    let w1 = v.stage_workspace(&h, 0).unwrap();
    let w2 = v.stage_workspace(&h, 0).unwrap();
    let mut s1 = v.stage_replace(&h, w1, l1, 3).unwrap();
    let mut s2 = v.stage_replace(&h, w2, l2, 3).unwrap();
    write(&mut s1, b"one");
    write(&mut s2, b"two");
    v.publish(&h, s1).unwrap();
    assert_eq!(v.publish(&h, s2), Err(ViewError::Conflict));
    assert_eq!(read(&mut v, &h, "k"), b"one");
    let old = v.item_lease(&h, key("k")).unwrap();
    v.replace(&h, key("k"), b"new").unwrap();
    let w = v.stage_workspace(&h, 0).unwrap();
    assert!(matches!(
        v.stage_replace(&h, w, old, 3),
        Err(ViewError::Conflict)
    ));
    assert_eq!(read(&mut v, &h, "k"), b"new");
}

#[test]
fn bp_e_aba_evict_reinsert_same_key_and_bytes_cannot_satisfy_old_identity() {
    let mut v = views(100_000, 10);
    let h = v.begin(target(1), epoch(1)).unwrap();
    v.insert(&h, Some(key("k")), b"old").unwrap();
    let lease = v.item_lease(&h, key("k")).unwrap();
    let w = v.stage_workspace(&h, 0).unwrap();
    let mut stage = v.stage_replace(&h, w, lease, 3).unwrap();
    write(&mut stage, b"bad");
    for _ in 0..10 {
        v.insert(&h, None, b"later").unwrap();
    }
    v.insert(&h, Some(key("k")), b"old").unwrap();
    assert_eq!(v.publish(&h, stage), Err(ViewError::Conflict));
    assert_eq!(read(&mut v, &h, "k"), b"old");
}

#[test]
fn bp_f_foreign_work_lease_stage_and_each_epoch_are_rejected_without_new_read_failure() {
    let mut a = views(100_000, 100);
    let mut b = views(100_000, 100);
    let old = a.begin(target(1), epoch(1)).unwrap();
    let foreign = b.begin(target(1), epoch(1)).unwrap();
    let w = a.stage_workspace(&old, 0).unwrap();
    assert!(matches!(
        b.stage_insert(&foreign, w, None, 1),
        Err(ViewError::Stale)
    ));
    a.insert(&old, Some(key("k")), b"x").unwrap();
    let lease = a.item_lease(&old, key("k")).unwrap();
    let work = b.stage_workspace(&foreign, 0).unwrap();
    assert!(matches!(
        b.stage_replace(&foreign, work, lease, 1),
        Err(ViewError::Stale)
    ));
    let resources = [
        epoch(1),
        ResourceEpoch {
            load: LoadEpoch(acp_remote::domain::EntityId::from_bytes([9; 16])),
            ..epoch(1)
        },
        ResourceEpoch {
            connection: HarnessConnectionEpoch(acp_remote::domain::EntityId::from_bytes([9; 16])),
            ..epoch(1)
        },
        epoch(9),
    ];
    let mut handle = old;
    for resource in resources {
        let w = a.stage_workspace(&handle, 0).unwrap();
        let mut stage = a.stage_insert(&handle, w, None, 1).unwrap();
        write(&mut stage, b"x");
        a.release(&handle).unwrap();
        let next = a.begin(target(1), resource).unwrap();
        assert_eq!(a.publish(&next, stage), Err(ViewError::Stale));
        assert_eq!(a.window_status(&next).unwrap().state, ViewState::Reading);
        handle = next;
    }
    assert_eq!(b.window_status(&foreign).unwrap().state, ViewState::Reading);
}

#[test]
fn bp_f_stage_drop_keeps_old_cell_charge_then_retires_it_without_polluting_new_snapshot() {
    let mut v = views(100_000, 100);
    let t = target(1);
    let old = v.begin(t.clone(), epoch(1)).unwrap();
    let snap = v.snapshot(&old).unwrap();
    let w = v.stage_workspace(&old, 0).unwrap();
    let stage = v.stage_insert(&old, w, None, 0).unwrap();
    v.release(&old).unwrap();
    let new = v.begin(t.clone(), epoch(2)).unwrap();
    assert_usage(&v, &t, 1025 + 2 * 1024 + 256 + 512 + 512, 1);
    drop(stage);
    assert_usage(&v, &t, 2049 + 256, 0);
    assert_eq!(v.window_status(&new).unwrap().state, ViewState::Reading);
    assert_eq!(snap.state(), ViewState::Reading);
    drop(snap);
    assert_usage(&v, &t, 2049, 0);
}

#[test]
fn bp_f_abort_reason_sticky_and_stale_abort_consumes_without_leaks() {
    let mut v = views(100_000, 100);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let w = v.stage_workspace(&h, 0).unwrap();
    let stage = v.stage_insert(&h, w, None, 0).unwrap();
    v.abort_stage(&h, stage, ViewFailure::ConnectionLost)
        .unwrap();
    assert_eq!(
        v.window_status(&h).unwrap().state,
        ViewState::Incomplete(ViewFailure::ConnectionLost)
    );
    v.fail(&h, ViewFailure::SourceRead).unwrap();
    assert_eq!(v.finish(&h), Err(ViewError::Incomplete));
    let new = v.begin(target(1), epoch(2)).unwrap();
    let w = v.stage_workspace(&new, 0).unwrap();
    let stage = v.stage_insert(&new, w, None, 0).unwrap();
    v.release(&new).unwrap();
    let latest = v.begin(target(1), epoch(3)).unwrap();
    assert_eq!(
        v.abort_stage(&new, stage, ViewFailure::SourceRead),
        Err(ViewError::Stale)
    );
    assert_eq!(v.window_status(&latest).unwrap().state, ViewState::Reading);
    assert_eq!(v.target_usage(&target(1)).unwrap().bytes, 2049);
}

#[test]
fn bp_g_duplicate_staged_inserts_revalidate_and_never_publish_half_items() {
    let mut v = views(100_000, 100);
    let h = v.begin(target(1), epoch(1)).unwrap();
    let w1 = v.stage_workspace(&h, 0).unwrap();
    let w2 = v.stage_workspace(&h, 0).unwrap();
    let mut first_stage = v.stage_insert(&h, w1, Some(key("k")), 1).unwrap();
    let mut second_stage = v.stage_insert(&h, w2, Some(key("k")), 1).unwrap();
    write(&mut first_stage, b"a");
    write(&mut second_stage, b"b");
    assert_eq!(v.snapshot(&h).unwrap().items().len(), 0);
    v.publish(&h, first_stage).unwrap();
    assert_eq!(v.publish(&h, second_stage), Err(ViewError::Conflict));
    assert_eq!(read(&mut v, &h, "k"), b"a");
    assert_eq!(v.window_status(&h).unwrap().next_position, Some(2));
    let before = v.window_status(&h).unwrap();
    let w = v.stage_workspace(&h, 0).unwrap();
    assert!(matches!(
        v.stage_insert(&h, w, Some(key("k")), 1),
        Err(ViewError::Conflict)
    ));
    assert_eq!(
        v.window_status(&h).unwrap().evicted_count,
        before.evicted_count
    );
}

#[test]
fn bp_g_fifo_long_stream_continues_status_works_when_snapshot_cannot_fit() {
    let mut v = views(4096, 100);
    let h = v.begin(target(1), epoch(1)).unwrap();
    for n in 1..=10000 {
        let w = v.stage_workspace(&h, 0).unwrap();
        let mut stage = v.stage_insert(&h, w, None, 1).unwrap();
        write(&mut stage, b"x");
        assert_eq!(v.publish(&h, stage).unwrap().position, n);
    }
    let status = v.window_status(&h).unwrap();
    assert_eq!(status.last_position, Some(10000));
    assert!(status.evicted_count > 9900);
    assert_eq!(status.next_position, Some(10001));
    v.finish(&h).unwrap();
    assert!(!v.snapshot(&h).unwrap().history_complete());
}

#[test]
fn bp_f_sixty_four_old_workspace_identities_refund_only_after_last_cell_ref() {
    let mut v = ViewWindows::new(ViewLimits::default()).unwrap();
    let mut old = Vec::new();
    for n in 0..64 {
        let h = v.begin(target(n), epoch(1)).unwrap();
        old.push(v.stage_workspace(&h, 0).unwrap());
        v.release(&h).unwrap();
    }
    assert_eq!(v.identity_count(), 64);
    assert!(matches!(
        v.begin(target(65), epoch(1)),
        Err(ViewError::Capacity)
    ));
    drop(old.pop());
    v.collect_released();
    assert_eq!(v.identity_count(), 63);
    let h = v.begin(target(65), epoch(1)).unwrap();
    drop(old);
    v.release(&h).unwrap();
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
fn bp_b_node_budget_and_cross_generation_target_peak_share_one_ledger() {
    let mut v = ViewWindows::new(ViewLimits {
        node_bytes: 8192 + 2 * 2049 + 8192,
        ..ViewLimits::default()
    })
    .unwrap();
    let a = v.begin(target(1), epoch(1)).unwrap();
    let b = v.begin(target(2), epoch(1)).unwrap();
    let w = v.stage_workspace(&a, 7680).unwrap();
    assert!(matches!(v.stage_workspace(&b, 0), Err(ViewError::Capacity)));
    drop(w);
    assert!(v.stage_workspace(&b, 0).is_ok());
    let mut v = views(2049 + 512 + 1024, 100);
    let old = v.begin(target(1), epoch(1)).unwrap();
    let w = v.stage_workspace(&old, 0).unwrap();
    v.release(&old).unwrap();
    let new = v.begin(target(1), epoch(2)).unwrap();
    assert!(matches!(
        v.stage_workspace(&new, 0),
        Err(ViewError::Capacity)
    ));
    drop(w);
    assert!(v.stage_workspace(&new, 0).is_ok());
}

#[test]
fn bp_f_same_container_wrong_target_rejects_and_only_original_stage_drop_marks_failure() {
    let mut views = views(100_000, 100);
    let first = views.begin(target(1), epoch(1)).unwrap();
    let second = views.begin(target(2), epoch(1)).unwrap();
    views.insert(&first, Some(key("k")), b"x").unwrap();
    let lease = views.item_lease(&first, key("k")).unwrap();
    let work = views.stage_workspace(&second, 0).unwrap();
    assert!(matches!(
        views.stage_replace(&second, work, lease, 1),
        Err(ViewError::Stale)
    ));
    let work = views.stage_workspace(&first, 0).unwrap();
    assert!(matches!(
        views.stage_insert(&second, work, None, 1),
        Err(ViewError::Stale)
    ));
    assert_eq!(
        views.window_status(&first).unwrap().state,
        ViewState::Reading
    );
    let work = views.stage_workspace(&first, 0).unwrap();
    let mut stage = views.stage_insert(&first, work, None, 1).unwrap();
    write(&mut stage, b"y");
    assert_eq!(views.publish(&second, stage), Err(ViewError::Stale));
    assert_eq!(
        views.window_status(&first).unwrap().state,
        ViewState::Incomplete(ViewFailure::Structure)
    );
    assert_eq!(
        views.window_status(&second).unwrap().state,
        ViewState::Reading
    );
    assert_eq!(
        views.target_usage(&target(1)).unwrap(),
        ViewUsage {
            bytes: 2049 + 514,
            items: 1
        }
    );
    assert_eq!(
        views.target_usage(&target(2)).unwrap(),
        ViewUsage {
            bytes: 2049,
            items: 0
        }
    );
}

#[test]
fn bp_a_only_lease_does_not_keep_window_charge_and_full_stage_drop_is_a_failed_read() {
    let mut views = views(100_000, 100);
    let first = views.begin(target(1), epoch(1)).unwrap();
    views.insert(&first, Some(key("k")), b"x").unwrap();
    let lease = views.item_lease(&first, key("k")).unwrap();
    views.release(&first).unwrap();
    assert_usage(&views, &target(1), 1025 + 514 + 320, 2);
    assert_eq!(lease.item().bytes, b"x");
    drop(lease);
    views.collect_released();
    assert_eq!(views.identity_count(), 0);
    let next = views.begin(target(1), epoch(2)).unwrap();
    let frozen = views.snapshot(&next).unwrap();
    let work = views.stage_workspace(&next, 0).unwrap();
    let mut stage = views.stage_insert(&next, work, None, 1).unwrap();
    write(&mut stage, b"x");
    drop(stage);
    assert_eq!(frozen.state(), ViewState::Reading);
    assert_eq!(
        views.window_status(&next).unwrap().state,
        ViewState::Incomplete(ViewFailure::Structure)
    );
    assert_eq!(views.window_status(&next).unwrap().retained_items, 0);
}

#[test]
fn bp_c_stage_key_4096_utf8_bytes_valid_4097_structure_failure_no_eviction() {
    let mut views = views(100_000, 100);
    let handle = views.begin(target(1), epoch(1)).unwrap();
    let valid = "é".repeat(2048);
    let work = views.stage_workspace(&handle, 0).unwrap();
    let mut stage = views
        .stage_insert(&handle, work, Some(key(&valid)), 1)
        .unwrap();
    write(&mut stage, b"x");
    views.publish(&handle, stage).unwrap();
    let lease = views.item_lease(&handle, key(&valid)).unwrap();
    assert_eq!(lease.item().key.unwrap().id, valid);
    drop(lease);
    let bad = valid + "x";
    let work = views.stage_workspace(&handle, 0).unwrap();
    assert!(matches!(
        views.stage_insert(&handle, work, Some(key(&bad)), 1),
        Err(ViewError::InvalidKey)
    ));
    assert_eq!(
        views.window_status(&handle).unwrap().state,
        ViewState::Incomplete(ViewFailure::Structure)
    );
    assert_eq!(views.window_status(&handle).unwrap().evicted_count, 0);
}
