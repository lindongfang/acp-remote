use super::*;
use crate::domain::{
    EntityId,
    native::{HarnessConnectionEpoch, LoadEpoch, RuntimeEpoch},
};

fn fixture(limits: ViewLimits) -> (ViewWindows, ViewHandle) {
    let target =
        NativeSessionKey::from_config(Some(EntityId::from_bytes([1; 16])), "native").unwrap();
    let epoch = ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([2; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([3; 16])),
        load: LoadEpoch(EntityId::from_bytes([4; 16])),
    };
    let mut windows = ViewWindows::new(limits).unwrap();
    let handle = windows.begin(target, epoch).unwrap();
    (windows, handle)
}
fn stamp<'h>(windows: &ViewWindows, handle: &'h ViewHandle) -> WindowReadStamp<'h> {
    windows
        .with_window(handle, |window| window.stamp())
        .unwrap()
}
fn key(id: &str) -> SourceKey<'_> {
    SourceKey {
        namespace: KeyNamespace::Tool,
        id,
    }
}

#[test]
fn full_window_is_readable_without_snapshot_reference_room() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    for _ in 0..4096 {
        windows.insert(&handle, None, b"x").unwrap();
    }
    let usage = windows.usage();
    assert!(matches!(
        windows.snapshot(&handle),
        Err(ViewError::Capacity)
    ));
    let read = windows
        .with_window(&handle, |window| {
            assert_eq!(window.status().retained_items, 4096);
            let mut items = window.items();
            for expected in 1..=4096 {
                assert_eq!(items.len(), 4097 - expected);
                let item = items.next().unwrap();
                assert_eq!(item.position, u64::try_from(expected).unwrap());
                assert_eq!(item.bytes, b"x");
            }
            assert!(items.next().is_none());
            window.stamp()
        })
        .unwrap();
    assert_eq!(read.revision(), 4096);
    assert_eq!(windows.usage(), usage);
    assert_eq!(windows.validate_read_stamp(&handle, &read), Ok(()));
}

#[test]
fn stamps_bind_complete_read_identity_and_allow_later_mutation() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    let (other, other_handle) = fixture(ViewLimits::default());
    let read = stamp(&windows, &handle);
    assert_eq!(windows.validate_read_stamp(&handle.clone(), &read), Ok(()));
    assert_eq!(
        other.validate_read_stamp(&other_handle, &read),
        Err(ViewError::Stale)
    );
    windows.insert(&handle, None, b"new").unwrap();
    assert_eq!(
        windows.validate_read_stamp(&handle, &read),
        Err(ViewError::ReadChanged)
    );
    windows.release(&handle).unwrap();
    let new = windows
        .begin(handle.target().clone(), handle.resource())
        .unwrap();
    assert_eq!(
        windows.validate_read_stamp(&new, &read),
        Err(ViewError::Stale)
    );
    assert_eq!(stamp(&windows, &new).revision(), 0);
}

#[test]
fn callback_drop_changes_live_state_but_header_stays_frozen() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    let work = windows.stage_workspace(&handle, 0).unwrap();
    let stage = windows.stage_insert(&handle, work, None, 1).unwrap();
    let result = windows.with_window(&handle, |window| {
        assert_eq!(window.status().state, ViewState::Reading);
        drop(stage);
        assert_eq!(window.status().state, ViewState::Reading);
        assert_eq!(
            windows.window_status(&handle).unwrap().state,
            ViewState::Incomplete(ViewFailure::Structure)
        );
        windows
            .with_window(&handle, |nested| assert_eq!(nested.stamp().revision(), 1))
            .unwrap();
    });
    assert_eq!(result, Err(ViewError::ReadChanged));
    let work = windows.stage_workspace(&handle, 0).unwrap();
    let stage = windows.stage_insert(&handle, work, None, 0).unwrap();
    assert_eq!(windows.with_window(&handle, |_| drop(stage)), Ok(()));
    assert_eq!(stamp(&windows, &handle).revision(), 1);
}

#[test]
fn equal_replacements_and_state_changes_advance_but_no_ops_do_not() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    windows.insert(&handle, Some(key("tool")), b"same").unwrap();
    windows.replace(&handle, key("tool"), b"same").unwrap();
    let lease = windows.item_lease(&handle, key("tool")).unwrap();
    let work = windows.stage_workspace(&handle, 0).unwrap();
    let mut stage = windows.stage_replace(&handle, work, lease, 4).unwrap();
    stage.parts().writer.write_all(b"same").unwrap();
    windows.publish(&handle, stage).unwrap();
    assert_eq!(stamp(&windows, &handle).revision(), 3);
    drop(windows.snapshot(&handle).unwrap());
    drop(windows.stage_workspace(&handle, 0).unwrap());
    assert_eq!(stamp(&windows, &handle).revision(), 3);
    windows.finish(&handle).unwrap();
    windows.finish(&handle).unwrap();
    assert_eq!(stamp(&windows, &handle).revision(), 4);
    windows.fail(&handle, ViewFailure::SourceRead).unwrap();
    windows.fail(&handle, ViewFailure::Structure).unwrap();
    assert_eq!(stamp(&windows, &handle).revision(), 5);
    assert_eq!(windows.finish(&handle), Err(ViewError::Incomplete));
}

#[test]
fn failed_reservation_keeps_each_real_fifo_change_in_revision() {
    let (mut windows, handle) = fixture(ViewLimits {
        target_items: 4,
        ..ViewLimits::default()
    });
    windows.insert(&handle, Some(key("a")), b"a").unwrap();
    windows.insert(&handle, Some(key("b")), b"b").unwrap();
    let leases = [
        windows.item_lease(&handle, key("a")).unwrap(),
        windows.item_lease(&handle, key("b")).unwrap(),
    ];
    let before = stamp(&windows, &handle);
    assert_eq!(
        windows.insert(&handle, None, b"full"),
        Err(ViewError::Capacity)
    );
    assert_eq!(stamp(&windows, &handle).revision(), before.revision() + 3);
    let state = windows.window_status(&handle).unwrap();
    assert_eq!(state.evicted_count, 2);
    assert_eq!(state.evicted_through, Some(2));
    assert_eq!(state.retained_items, 0);
    assert_eq!(leases[0].item().bytes, b"a");
}

#[test]
fn revision_exhaustion_seals_before_content_changes_and_still_allows_diagnostics() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    windows.insert(&handle, Some(key("a")), b"a").unwrap();
    windows
        .set_revision_boundary_for_test(&handle, RevisionBoundary::Penultimate)
        .unwrap();
    windows.replace(&handle, key("a"), b"b").unwrap();
    let read = stamp(&windows, &handle);
    assert_eq!(read.revision(), u64::MAX);
    assert_eq!(
        windows.set_revision_boundary_for_test(&handle, RevisionBoundary::Penultimate),
        Err(ViewError::Conflict)
    );
    assert_eq!(
        windows.insert(&handle, None, b"never"),
        Err(ViewError::SequenceExhausted)
    );
    assert_eq!(
        windows.validate_read_stamp(&handle, &read),
        Err(ViewError::SequenceExhausted)
    );
    assert_eq!(
        windows.with_window(&handle, |_| panic!("sealed callback")),
        Err(ViewError::SequenceExhausted)
    );
    assert_eq!(windows.finish(&handle), Err(ViewError::SequenceExhausted));
    assert_eq!(
        windows.fail(&handle, ViewFailure::SourceRead),
        Err(ViewError::SequenceExhausted)
    );
    assert_eq!(
        windows.replace(&handle, key("missing"), b"x"),
        Err(ViewError::SequenceExhausted)
    );
    let snapshot = windows.snapshot(&handle).unwrap();
    assert_eq!(snapshot.items().len(), 1);
    assert_eq!(snapshot.items().next().unwrap().bytes, b"b");
    assert_eq!(
        snapshot.state(),
        ViewState::Incomplete(ViewFailure::SequenceExhausted)
    );
    assert_eq!(
        windows.item_lease(&handle, key("a")).unwrap().item().bytes,
        b"b"
    );
    assert!(matches!(
        windows.item_lease(&handle, key("missing")),
        Err(ViewError::MissingItem)
    ));
    assert!(matches!(
        windows.stage_workspace(&handle, 0),
        Err(ViewError::SequenceExhausted)
    ));
    windows.release(&handle).unwrap();
}

#[test]
fn maximum_revision_no_ops_do_not_seal_or_replace_first_failure() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    windows.finish(&handle).unwrap();
    windows
        .set_revision_boundary_for_test(&handle, RevisionBoundary::Last)
        .unwrap();
    windows.finish(&handle).unwrap();
    assert_eq!(stamp(&windows, &handle).revision(), u64::MAX);
    assert_eq!(
        windows.fail(&handle, ViewFailure::SourceRead),
        Err(ViewError::SequenceExhausted)
    );
    let (mut windows, handle) = fixture(ViewLimits::default());
    windows.fail(&handle, ViewFailure::ConnectionLost).unwrap();
    windows
        .set_revision_boundary_for_test(&handle, RevisionBoundary::Last)
        .unwrap();
    windows.fail(&handle, ViewFailure::Structure).unwrap();
    assert_eq!(stamp(&windows, &handle).revision(), u64::MAX);
    assert_eq!(windows.finish(&handle), Err(ViewError::Incomplete));
    assert_eq!(
        windows.insert(&handle, None, b"never"),
        Err(ViewError::SequenceExhausted)
    );
    assert_eq!(
        windows.window_status(&handle).unwrap().state,
        ViewState::Incomplete(ViewFailure::ConnectionLost)
    );
}

#[test]
fn sealed_stage_parameter_priority_keeps_identity_and_invalid_key_checks() {
    let (mut windows, handle) = fixture(ViewLimits::default());
    let bad_key_work = windows.stage_workspace(&handle, 0).unwrap();
    let cost_work = windows.stage_workspace(&handle, 0).unwrap();
    windows
        .set_revision_boundary_for_test(&handle, RevisionBoundary::Last)
        .unwrap();
    assert_eq!(
        windows.fail(&handle, ViewFailure::Structure),
        Err(ViewError::SequenceExhausted)
    );
    let bad = "x".repeat(4097);
    assert!(matches!(
        windows.stage_insert(&handle, bad_key_work, Some(key(&bad)), 0),
        Err(ViewError::InvalidKey)
    ));
    assert!(matches!(
        windows.stage_insert(&handle, cost_work, None, usize::MAX),
        Err(ViewError::SequenceExhausted)
    ));
    assert_eq!(
        windows.window_status(&handle).unwrap().state,
        ViewState::Incomplete(ViewFailure::SequenceExhausted)
    );
}
