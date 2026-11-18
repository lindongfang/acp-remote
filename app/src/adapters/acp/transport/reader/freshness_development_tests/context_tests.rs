use super::*;
use crate::{
    adapters::acp::{
        RawMessage, Response, UpdateFreshness, UpdateOrigin, ViewReadContextKind,
        native_dispatch::AcpNativeAdapter,
    },
    application::native_dispatch::{CapacityFreshness, CapacityUpdate, NativeLifecyclePort},
    domain::native_dispatch::ProtocolVersion,
};

fn assert_update(lease: &ObservationLease, expected: UpdateFreshness, origin: UpdateOrigin) {
    let received = lease.received();
    assert_eq!(received.update_freshness(), Some(expected));
    assert!(
        matches!(received.incoming(), Incoming::Update { origin: actual, .. } if *actual == origin)
    );
    let observation = AcpNativeAdapter::default()
        .capacity_observation(lease, ProtocolVersion::V2)
        .expect("actual adapter port")
        .expect("update projected");
    assert_eq!(
        observation.freshness,
        match expected {
            UpdateFreshness::Current => CapacityFreshness::Current,
            UpdateFreshness::History => CapacityFreshness::History,
            UpdateFreshness::Unconfirmed => CapacityFreshness::Unconfirmed,
        }
    );
}

#[tokio::test]
async fn freshness_same_cut_marker_changes_state_tool_and_child_before_response() {
    let mut fixture = Fixture::new(Version::V2).await;
    let id = fixture.read_request("s", true).await;
    let mut tool = update("s", "tool_call_update");
    tool["params"]["update"]["toolCallId"] = json!("tool");
    tool["params"]["update"]["status"] = json!("in_progress");
    let mut child = update("s", "subagent_update");
    child["params"]["update"]["sessionId"] = json!("child");
    let frames = [
        update("s", "state_update"),
        update("s", "replay_complete"),
        update("s", "state_update"),
        tool,
        child,
        json!({"jsonrpc":"2.0","id":id.json(),"result":{}}),
        update("s", "state_update"),
    ];
    fixture.feed(&frames).await;
    let mut receipt = None;
    let mut cell = None;
    for (index, frame) in frames.iter().enumerate() {
        let lease = fixture.next().await.expect("ordered lease");
        let received = lease.received();
        assert_eq!(received.raw().bytes(), serde_json::to_vec(frame).unwrap());
        assert_eq!(
            received.receipt_order(),
            *receipt.get_or_insert_with(|| received.receipt_order()),
            "this test must exercise one actual read cut"
        );
        match index {
            0..=4 => {
                assert_update(
                    &lease,
                    if index < 2 {
                        UpdateFreshness::History
                    } else {
                        UpdateFreshness::Current
                    },
                    UpdateOrigin::DuringReplay,
                );
                assert_eq!(
                    received.view_read_context(),
                    ViewReadContextKind::UniqueReceiptWindow
                );
                let window = received.view_read_window().expect("original window");
                assert_eq!(
                    window.cell_key(),
                    *cell.get_or_insert_with(|| window.cell_key())
                );
                assert_eq!(window.rpc(), &id);
                let observation = AcpNativeAdapter::default()
                    .capacity_observation(&lease, ProtocolVersion::V2)
                    .unwrap()
                    .unwrap();
                match index {
                    3 => assert!(matches!(observation.update, CapacityUpdate::Tool { .. })),
                    4 => assert!(matches!(observation.update, CapacityUpdate::Child { .. })),
                    _ => {}
                }
            }
            5 => {
                assert_eq!(received.update_freshness(), None);
                assert!(matches!(
                    received.incoming(),
                    Incoming::Response {
                        response: Response::Replayed { .. },
                        ..
                    }
                ));
            }
            _ => {
                assert_update(&lease, UpdateFreshness::Current, UpdateOrigin::Live);
                assert_eq!(
                    received.view_read_context(),
                    ViewReadContextKind::OutsideReadWindow
                );
                assert!(received.view_read_window().is_none());
            }
        }
        fixture.ack.acknowledge(lease).expect("ack");
    }
    fixture.stop().await;
}

#[tokio::test]
async fn freshness_cross_cut_other_session_marker_does_not_complete_replay() {
    let mut fixture = Fixture::new(Version::V2).await;
    let id = fixture.read_request("s", true).await;
    let mut receipts = Vec::new();
    for (session, kind, expected, origin) in [
        (
            "s",
            "state_update",
            UpdateFreshness::History,
            UpdateOrigin::DuringReplay,
        ),
        (
            "other",
            "replay_complete",
            UpdateFreshness::Current,
            UpdateOrigin::Live,
        ),
        (
            "s",
            "state_update",
            UpdateFreshness::History,
            UpdateOrigin::DuringReplay,
        ),
        (
            "s",
            "replay_complete",
            UpdateFreshness::History,
            UpdateOrigin::DuringReplay,
        ),
        (
            "s",
            "state_update",
            UpdateFreshness::Current,
            UpdateOrigin::DuringReplay,
        ),
    ] {
        fixture.feed(&[update(session, kind)]).await;
        let lease = fixture.next().await.expect("update");
        receipts.push(lease.received().receipt_order());
        assert_update(&lease, expected, origin);
        fixture.ack.acknowledge(lease).expect("ack");
    }
    assert!(receipts.windows(2).all(|pair| pair[0] < pair[1]));
    fixture
        .feed(&[json!({"jsonrpc":"2.0","id":id.json(),"result":{}})])
        .await;
    let lease = fixture.next().await.expect("success after current");
    fixture.ack.acknowledge(lease).expect("ack response");
    fixture.stop().await;
}

#[tokio::test]
async fn freshness_resume_without_cursor_is_current_without_complete() {
    let mut fixture = Fixture::new(Version::V2).await;
    let id = fixture.read_request("s", false).await;
    fixture
        .feed(&[
            update("s", "state_update"),
            json!({"jsonrpc":"2.0","id":id.json(),"result":{}}),
        ])
        .await;
    let lease = fixture.next().await.expect("snapshot");
    assert_update(&lease, UpdateFreshness::Current, UpdateOrigin::DuringResume);
    fixture.ack.acknowledge(lease).expect("ack snapshot");
    let lease = fixture.next().await.expect("resumed");
    assert!(matches!(
        lease.received().incoming(),
        Incoming::Response {
            response: Response::Resumed { .. },
            ..
        }
    ));
    fixture.ack.acknowledge(lease).expect("ack response");
    fixture.stop().await;
}

#[tokio::test]
async fn freshness_v1_resume_and_load_windows_remain_history_even_after_marker() {
    for replay in [false, true] {
        let mut fixture = Fixture::new(Version::V1).await;
        let id = fixture.read_request("s", replay).await;
        let frames = [
            update("s", "state_update"),
            update("s", "replay_complete"),
            update("s", "state_update"),
            json!({"jsonrpc":"2.0","id":id.json(),"result":{}}),
        ];
        fixture.feed(&frames).await;
        for _ in 0..3 {
            let lease = fixture.next().await.expect("v1 update");
            assert_eq!(
                lease.received().update_freshness(),
                Some(UpdateFreshness::History)
            );
            let observation = AcpNativeAdapter::default()
                .capacity_observation(&lease, ProtocolVersion::V1)
                .unwrap()
                .unwrap();
            assert_eq!(observation.freshness, CapacityFreshness::History);
            assert_eq!(observation.update, CapacityUpdate::Activity);
            fixture.ack.acknowledge(lease).expect("ack");
        }
        let lease = fixture.next().await.expect("v1 read response");
        fixture.ack.acknowledge(lease).expect("ack response");
        fixture.stop().await;
    }
}

#[tokio::test]
async fn freshness_concurrent_sessions_and_failed_replay_keep_separate_phases() {
    let mut fixture = Fixture::new(Version::V2).await;
    let a = fixture.read_request("a", true).await;
    let b = fixture.read_request("b", true).await;
    fixture
        .feed(&[
            update("a", "replay_complete"),
            update("a", "state_update"),
            update("b", "state_update"),
            json!({"jsonrpc":"2.0","id":a.json(),"result":{}}),
            json!({"jsonrpc":"2.0","id":b.json(),"error":{"code":-32000,"message":"fixture"}}),
        ])
        .await;
    for expected in [
        UpdateFreshness::History,
        UpdateFreshness::Current,
        UpdateFreshness::History,
    ] {
        let lease = fixture.next().await.expect("update");
        assert_update(&lease, expected, UpdateOrigin::DuringReplay);
        fixture.ack.acknowledge(lease).expect("ack");
    }
    for _ in 0..2 {
        let lease = fixture.next().await.expect("response");
        fixture.ack.acknowledge(lease).expect("ack response");
    }
    fixture.stop().await;
}

fn route_fixture(
    fixture: &Fixture,
    cut: &super::super::ReceiptCut,
    frame: &Value,
) -> Result<(), AcpError> {
    super::super::route(
        &fixture.connection.core,
        cut,
        RawMessage::parse(serde_json::to_vec(frame).unwrap()).unwrap(),
    )
}

#[tokio::test]
async fn freshness_old_cut_marker_and_response_do_not_advance_retry_key() {
    let mut fixture = Fixture::new(Version::V2).await;
    let old = fixture.read_request("s", true).await;
    // A synthetic held cut deterministically interleaves the actual public retry between frames.
    let cut = fixture
        .connection
        .core
        .lock()
        .capture(Instant::now(), 0)
        .unwrap();
    let response =
        json!({"jsonrpc":"2.0","id":old.json(),"error":{"code":-32000,"message":"fixture"}});
    route_fixture(&fixture, &cut, &response).unwrap();
    let lease = fixture.next().await.expect("old response");
    fixture.ack.acknowledge(lease).expect("ack old response");
    let new = fixture.read_request("s", true).await;
    assert_ne!(old, new);
    route_fixture(&fixture, &cut, &update("s", "replay_complete")).unwrap();
    let lease = fixture.next().await.expect("stale marker");
    assert_update(&lease, UpdateFreshness::Unconfirmed, UpdateOrigin::Live);
    fixture.ack.acknowledge(lease).expect("ack stale marker");
    assert_eq!(
        route_fixture(&fixture, &cut, &response),
        Err(AcpError::UnmatchedResponse)
    );
    fixture
        .feed(&[
            update("s", "state_update"),
            update("s", "replay_complete"),
            update("s", "state_update"),
            json!({"jsonrpc":"2.0","id":new.json(),"result":{}}),
        ])
        .await;
    for expected in [
        UpdateFreshness::History,
        UpdateFreshness::History,
        UpdateFreshness::Current,
    ] {
        let lease = fixture.next().await.expect("new key update");
        assert_update(&lease, expected, UpdateOrigin::DuringReplay);
        fixture.ack.acknowledge(lease).expect("ack");
    }
    let lease = fixture.next().await.expect("new response");
    fixture.ack.acknowledge(lease).expect("ack response");
    fixture.stop().await;
}

#[tokio::test]
async fn freshness_ambiguous_marker_advances_no_cell_and_missing_metadata_is_unconfirmed() {
    let mut fixture = Fixture::new(Version::V2).await;
    let a = fixture.read_request("a", true).await;
    let b = fixture.read_request("b", true).await;
    let cut = {
        let mut state = fixture.connection.core.lock();
        // Public reservation forbids this ambiguity; exercise the defensive receipt boundary.
        state.pending.get_mut(&b).unwrap().kind =
            crate::adapters::acp::RequestKind::Replay(parse_session(&json!("a")).unwrap());
        state.capture(Instant::now(), 0).unwrap()
    };
    route_fixture(&fixture, &cut, &update("a", "replay_complete")).unwrap();
    let lease = fixture.next().await.expect("ambiguous marker");
    assert_eq!(
        lease.received().view_read_context(),
        ViewReadContextKind::AmbiguousReceiptWindow
    );
    assert_eq!(
        lease.received().update_freshness(),
        Some(UpdateFreshness::Unconfirmed)
    );
    fixture.ack.acknowledge(lease).expect("ack");
    {
        let state = fixture.connection.core.lock();
        assert!(!state.pending[&a].replay_complete);
        assert!(!state.pending[&b].replay_complete);
    }
    route_fixture(&fixture, &cut, &update("a", "state_update")).unwrap();
    fixture
        .connection
        .core
        .lock()
        .events
        .back_mut()
        .unwrap()
        .update_freshness = None;
    let lease = fixture.next().await.expect("metadata absent");
    let observation = AcpNativeAdapter::default()
        .capacity_observation(&lease, ProtocolVersion::V2)
        .unwrap()
        .unwrap();
    assert_eq!(observation.freshness, CapacityFreshness::Unconfirmed);
    fixture.ack.acknowledge(lease).expect("ack");
    fixture.stop().await;
}
