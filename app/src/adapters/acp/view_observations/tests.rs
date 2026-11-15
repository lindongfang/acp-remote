use super::super::test_support::*;
use super::*;
use crate::domain::{
    EntityId,
    native::{HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch},
    views::{ViewLimits, ViewState},
};
use serde_json::json;

fn target() -> NativeSessionKey {
    NativeSessionKey::from_config(Some(EntityId::from_bytes([1; 16])), "s").unwrap()
}
fn epoch(n: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([1; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([2; 16])),
        load: LoadEpoch(EntityId::from_bytes([n; 16])),
    }
}
fn strategy(version: Version) -> Strategy {
    unit_strategy(version)
}
fn update(strategy: &Strategy, raw: &[u8], origin: UpdateOrigin) -> Incoming {
    let raw = RawMessage::parse(raw.to_vec()).unwrap();
    let event = strategy
        .update(raw.field("params").expect("unique decoded field").unwrap())
        .unwrap();
    Incoming::Update { event, origin, raw }
}
fn frame(session: &str, tag: &str, text: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"jsonrpc":"2.0","method":"session/update","params":{
        "sessionId":session,"update":{"sessionUpdate":tag,"state":"idle","opaque":text}}}))
    .unwrap()
}
fn window(bytes: usize) -> (ViewWindows, ViewHandle) {
    let mut windows = ViewWindows::new(ViewLimits {
        target_bytes: bytes,
        target_items: 32,
        ..ViewLimits::default()
    })
    .unwrap();
    let handle = windows.begin(target(), epoch(1)).unwrap();
    (windows, handle)
}

#[test]
fn raw_header_preserves_each_origin_and_exact_json_bytes() {
    let raw = br#" { "jsonrpc":"2.0", "method":"session/update", "params":{"sessionId":"s","update":{"sessionUpdate":"future_update","v":1e+02,"s":"\u0000"}}, "_meta":{"unknown":[null,false]}} "#;
    for version in [Version::V1, Version::V2] {
        let selected = strategy(version);
        let (mut windows, handle) = window(65536);
        let before = windows.target_usage(&target()).unwrap();
        let mut read = ObservationRead::begin(&mut windows, &handle, &selected).unwrap();
        assert_eq!(
            windows.target_usage(&target()).unwrap().bytes,
            before.bytes + 1536
        );
        for origin in [
            UpdateOrigin::DuringReplay,
            UpdateOrigin::Live,
            UpdateOrigin::DuringResume,
        ] {
            let incoming = update(&selected, raw, origin);
            assert!(matches!(
                read.observe_inner(&mut windows, &incoming).unwrap(),
                ObservationOutcome::Recorded { .. }
            ));
        }
        let snapshot = windows.snapshot(&handle).unwrap();
        for (index, item) in snapshot.items().enumerate() {
            assert!(item.key.is_none());
            let decoded = parse_header(item.bytes).unwrap();
            assert_eq!(decoded.raw, raw);
            assert_eq!(decoded.sequence, u64::try_from(index).unwrap() + 1);
            assert_eq!(decoded.version, version);
            assert_eq!(
                decoded.origin,
                [
                    UpdateOrigin::DuringReplay,
                    UpdateOrigin::Live,
                    UpdateOrigin::DuringResume
                ][index]
            );
        }
        assert_eq!(read.stats().last_sequence, 3);
        assert_eq!(read.stats().dropped_count, 0);
    }
}

#[test]
fn control_other_target_and_non_updates_are_not_display_loss_or_sequences() {
    for version in [Version::V1, Version::V2] {
        let selected = strategy(version);
        let (mut windows, handle) = window(65536);
        let mut read = ObservationRead::begin(&mut windows, &handle, &selected).unwrap();
        let before = windows.usage();
        let other = update(
            &selected,
            &frame("other", "future_update", "data"),
            UpdateOrigin::Live,
        );
        assert_eq!(
            read.observe_inner(&mut windows, &other).unwrap(),
            ObservationOutcome::Ignored
        );
        let unknown = Incoming::UnknownNotification {
            raw: RawMessage::parse(br#"{"jsonrpc":"2.0","method":"_future"}"#.to_vec()).unwrap(),
        };
        assert_eq!(
            read.observe_inner(&mut windows, &unknown).unwrap(),
            ObservationOutcome::Ignored
        );
        assert_eq!(read.stats(), ObservationStats::default());
        assert_eq!(windows.usage(), before);
        let state = update(
            &selected,
            &frame("s", "state_update", "data"),
            UpdateOrigin::Live,
        );
        if version == Version::V2 {
            assert_eq!(
                read.observe_inner(&mut windows, &state).unwrap(),
                ObservationOutcome::Ignored
            );
            assert_eq!(windows.usage(), before);
        } else {
            assert!(matches!(
                read.observe_inner(&mut windows, &state).unwrap(),
                ObservationOutcome::Recorded { sequence: 1, .. }
            ));
        }
    }
}

#[test]
fn one_oversize_display_frame_does_not_stop_later_small_records() {
    let selected = strategy(Version::V1);
    let (mut windows, handle) = window(8000);
    let mut read = ObservationRead::begin(&mut windows, &handle, &selected).unwrap();
    let large = update(
        &selected,
        &frame("s", "future_update", &"x".repeat(10000)),
        UpdateOrigin::DuringReplay,
    );
    assert_eq!(
        read.observe_inner(&mut windows, &large).unwrap(),
        ObservationOutcome::Dropped {
            sequence: 1,
            reason: ObservationFailure::Capacity
        }
    );
    assert_eq!(read.stats().first_dropped, Some(1));
    assert_eq!(read.stats().last_dropped, Some(1));
    let small = update(
        &selected,
        &frame("s", "future_update", "small"),
        UpdateOrigin::Live,
    );
    assert!(matches!(
        read.observe_inner(&mut windows, &small).unwrap(),
        ObservationOutcome::Recorded { sequence: 2, .. }
    ));
    assert_eq!(windows.finish(&handle), Err(ViewError::Incomplete));
    let snapshot = windows.snapshot(&handle).unwrap();
    assert_eq!(snapshot.items().len(), 1);
    assert_eq!(
        parse_header(snapshot.items().next().unwrap().bytes)
            .unwrap()
            .sequence,
        2
    );
}

#[test]
fn raw_fifo_keeps_the_recent_window_in_a_long_stream() {
    let selected = strategy(Version::V1);
    let (mut windows, handle) = window(8000);
    let mut read = ObservationRead::begin(&mut windows, &handle, &selected).unwrap();
    let incoming = update(
        &selected,
        &frame("s", "future_update", "same bytes are distinct observations"),
        UpdateOrigin::Live,
    );
    for sequence in 1..=10000 {
        assert!(
            matches!(read.observe_inner(&mut windows, &incoming).unwrap(), ObservationOutcome::Recorded { sequence: observed, .. } if sequence==observed)
        );
    }
    let status = windows.window_status(&handle).unwrap();
    assert_eq!(status.last_position, Some(10000));
    assert!(status.evicted_count > 9900);
    let snapshot = windows.snapshot(&handle).unwrap();
    let last = snapshot.items().last().unwrap();
    assert_eq!(parse_header(last.bytes).unwrap().sequence, 10000);
    assert!(!snapshot.history_complete());
}

#[test]
fn old_observer_charge_survives_release_and_stale_input_cannot_poison_new_read() {
    let selected = strategy(Version::V1);
    let (mut windows, handle) = window(65536);
    let mut read = ObservationRead::begin(&mut windows, &handle, &selected).unwrap();
    windows.release(&handle).unwrap();
    let new = windows.begin(target(), epoch(2)).unwrap();
    assert_eq!(
        windows.target_usage(&target()).unwrap().bytes,
        2049 + 1536 + 1024
    );
    let incoming = update(
        &selected,
        &frame("s", "future_update", "data"),
        UpdateOrigin::Live,
    );
    assert_eq!(
        read.observe_inner(&mut windows, &incoming),
        Err(ObservationError::View(ViewError::Stale))
    );
    assert_eq!(read.stats().last_sequence, 0);
    assert_eq!(
        windows.window_status(&new).unwrap().state,
        ViewState::Reading
    );
    drop(read);
    assert_eq!(windows.target_usage(&target()).unwrap().bytes, 2049);
}

#[test]
fn inconsistent_projection_is_rejected_before_display_accounting() {
    let selected = strategy(Version::V1);
    let (mut windows, handle) = window(65536);
    let mut read = ObservationRead::begin(&mut windows, &handle, &selected).unwrap();
    let mut incoming = update(
        &selected,
        &frame("s", "future_update", "data"),
        UpdateOrigin::Live,
    );
    if let Incoming::Update { event, .. } = &mut incoming {
        *event = SessionEvent::Other {
            session: parse_session(&json!("s")).unwrap(),
            kind: "different-from-raw".into(),
        };
    }
    let before = windows.usage();
    assert_eq!(
        read.observe_inner(&mut windows, &incoming),
        Err(ObservationError::InvalidObservation)
    );
    assert_eq!(windows.usage(), before);
    assert_eq!(read.stats(), ObservationStats::default());
}

#[test]
fn ignored_events_and_matching_stale_reads_take_precedence_over_raw_consistency() {
    let selected = strategy(Version::V2);
    let (mut windows, handle) = window(65536);
    let mut read = ObservationRead::begin(&mut windows, &handle, &selected).unwrap();
    let mut other = update(
        &selected,
        &frame("s", "future_update", "data"),
        UpdateOrigin::Live,
    );
    if let Incoming::Update {
        event: SessionEvent::Other { session, .. },
        ..
    } = &mut other
    {
        *session = parse_session(&json!("other")).unwrap();
    }
    let before = windows.usage();
    assert_eq!(
        read.observe_inner(&mut windows, &other).unwrap(),
        ObservationOutcome::Ignored
    );
    let mut state = update(
        &selected,
        &frame("s", "state_update", "data"),
        UpdateOrigin::Live,
    );
    if let Incoming::Update {
        event: SessionEvent::State { state, .. },
        ..
    } = &mut state
    {
        *state = "different-from-raw".into();
    }
    assert_eq!(
        read.observe_inner(&mut windows, &state).unwrap(),
        ObservationOutcome::Ignored
    );
    assert_eq!(windows.usage(), before);
    assert_eq!(read.stats(), ObservationStats::default());
    let mut matching = update(
        &selected,
        &frame("s", "future_update", "data"),
        UpdateOrigin::Live,
    );
    if let Incoming::Update {
        event: SessionEvent::Other { kind, .. },
        ..
    } = &mut matching
    {
        *kind = "different-from-raw".into();
    }
    windows.release(&handle).unwrap();
    let new = windows.begin(target(), epoch(2)).unwrap();
    assert_eq!(
        read.observe_inner(&mut windows, &matching),
        Err(ObservationError::View(ViewError::Stale))
    );
    assert_eq!(read.stats(), ObservationStats::default());
    assert_eq!(
        windows.window_status(&new).unwrap().state,
        ViewState::Reading
    );
}

#[test]
fn counter_overflow_only_closes_this_display_read_without_wrapping() {
    let selected = strategy(Version::V1);
    let (mut windows, handle) = window(8000);
    let mut read = ObservationRead::begin(&mut windows, &handle, &selected).unwrap();
    read.stats.last_sequence = u64::MAX;
    let incoming = update(
        &selected,
        &frame("s", "future_update", "data"),
        UpdateOrigin::Live,
    );
    assert_eq!(
        read.observe_inner(&mut windows, &incoming),
        Err(ObservationError::CounterExhausted)
    );
    assert_eq!(read.stats().last_sequence, u64::MAX);
    assert!(read.stats().closed);
    assert_eq!(windows.window_status(&handle).unwrap().retained_items, 0);
    let next = windows.begin(target(), epoch(2)).unwrap();
    let mut fresh = ObservationRead::begin(&mut windows, &next, &selected).unwrap();
    fresh.stats.dropped_count = u64::MAX;
    let large = update(
        &selected,
        &frame("s", "future_update", &"x".repeat(10000)),
        UpdateOrigin::Live,
    );
    assert_eq!(
        fresh.observe_inner(&mut windows, &large),
        Err(ObservationError::CounterExhausted)
    );
    assert_eq!(fresh.stats().dropped_count, u64::MAX);
    assert!(fresh.stats().closed);
}

#[test]
fn header_decoder_is_strict_borrowed_and_does_not_claim_json_validation() {
    let mut bytes = [0_u8; 33];
    bytes[..4].copy_from_slice(b"ACPO");
    bytes[4..8].copy_from_slice(&[1, 2, 3, 1]);
    bytes[8..16].copy_from_slice(&1_u64.to_le_bytes());
    bytes[16..24].copy_from_slice(&1_u64.to_le_bytes());
    bytes[32] = b'x';
    let decoded = parse_header(&bytes).unwrap();
    assert_eq!(decoded.raw.as_ptr(), bytes[32..].as_ptr());
    assert_eq!(decoded.raw, b"x"); // Format-only: x is not a JSON-RPC message.
    for offset in [0, 4, 5, 6, 7, 8, 16, 24] {
        let mut invalid = bytes;
        invalid[offset] = if matches!(offset, 8 | 16) { 0 } else { 255 };
        assert_eq!(
            parse_header(&invalid).unwrap_err(),
            ObservationError::InvalidHeader
        );
    }
    let mut zero_origin = bytes;
    zero_origin[6] = 0;
    assert!(parse_header(&zero_origin).is_err());
    assert!(parse_header(&bytes[..32]).is_err());
    let mut tail = bytes.to_vec();
    tail.push(0);
    assert!(parse_header(&tail).is_err());
    assert!(std::mem::size_of::<ObservationRead>() <= 512);
    println!(
        "ObservationRead={} Stats={} RawObservation={}",
        std::mem::size_of::<ObservationRead>(),
        std::mem::size_of::<ObservationStats>(),
        std::mem::size_of::<RawObservation<'_>>()
    );
}
