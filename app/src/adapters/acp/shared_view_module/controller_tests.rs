use super::*;
use crate::{
    adapters::acp::{Version, WriteProgress, WriteReport, test_support::setup},
    application::shared_views::{
        CurrentViewAuthority,
        controller::{HookSelector, ViewFalliblePoint, ViewHookPoint},
        local_transfer::LocalCreditResult,
        reader::PrepareNext,
        wire::{HEADER_BYTES, TransferHeader},
    },
    domain::{
        EntityId,
        shared_view::{ProjectionState, ReadMode},
    },
    shared_view_test_support::{FixtureSigningClient, FixtureTaskOwner, OwnedTestArea},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
mod reachability_tests;
struct Harness {
    auth: Arc<LocalViewAuthFixture>,
    owner: FixtureTaskOwner,
    module: AcpViewModuleFixture,
    target: FixtureTarget,
    peer: crate::shared_view_test_support::FixturePeer,
    client: AuthenticatedLocalViewClient,
}

#[tokio::test]
async fn shared_view_actual_never_enqueued_zero_retires_its_original_coverage_and_next_read_recovers()
 {
    use crate::application::shared_views::{
        budget::ReadReason,
        read_control::{NativeReadControl, ReadTransportState},
    };
    let mut harness = Harness::start().await;
    let before = harness.module.core.enqueues.load(Ordering::Acquire);
    let first = harness
        .module
        .replay(
            &harness.target,
            setup(Version::V2, "C:\\fixture"),
            NativeReadOptions {
                write_deadline: Instant::now() - Duration::from_millis(1),
                response_wait: ReadResponseWait::Unbounded,
            },
        )
        .await
        .expect("explicit old IO deadline reaches original enqueue path");
    let mut changes = first.subscribe();
    timeout_at(deadline(),async {loop {
        let signal=harness.module.views().core.changed.clone();let wake=signal.notified();tokio::pin!(wake);wake.as_mut().enable();
        let current=harness.module.views().core.target(harness.target.key()).expect("actual target");let snapshot=current.snapshot.read().expect("snapshot").clone();
        let state=harness.module.native_reads.status(&first);
        if state.transport()==ReadTransportState::Zero&&state.read_mode()==ReadMode::ReplayIncomplete&&snapshot.segments.publications.len()==2 {break;}
        tokio::select!{()=wake=>{},result=changes.changed()=>{result.expect("original read metadata");}}
    }}).await.expect("actual original Zero and matching failure candidate converge");
    assert_eq!(
        harness.module.core.enqueues.load(Ordering::Acquire),
        before,
        "the first original reservation never reaches the wire"
    );
    assert!(!harness.module.core.connection.is_closed());
    let _next = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::FirstOpen)
        .expect("fresh original reservation");
    harness.complete_replay().await;
    harness.wait_complete().await;
    assert_eq!(
        harness.module.core.enqueues.load(Ordering::Acquire),
        before + 1
    );
    assert_eq!(
        harness.module.policy.calls.load(Ordering::Acquire),
        1,
        "only the successor uses the service policy"
    );
    harness.close().await;
}

#[allow(
    clippy::too_many_lines,
    reason = "Two real NativeSessionIds share the original connection and expose exact per-cell gap attribution"
)]
#[tokio::test]
async fn shared_view_actual_other_sid_capture_gap_cannot_poison_a_different_real_accepted_window() {
    use crate::application::shared_views::{budget::ReadReason, read_control::NativeReadControl};
    let mut harness = Harness::start().await;
    let mut peer = std::mem::replace(&mut harness.peer, {
        let (wire, peer) =
            FixturePeerWire::from_duplex(&harness.owner.handle(), tokio::io::duplex(16))
                .unwrap_or_else(|_| panic!("owned spare endpoints"));
        drop(wire);
        peer
    });
    let driver = tokio::spawn(async move {
        super::tests::reply(&mut peer, br#"{"sessionId":"sibling-session"}"#).await;
        peer
    });
    let sibling = harness
        .module
        .create(setup(Version::V2, "C:\\fixture"))
        .await
        .expect("second real NewSession typed-RPC/SID binding");
    harness.peer = driver.await.expect("original peer owner joins");
    assert_eq!(sibling.scope(), harness.module.scope());
    let first = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::FirstOpen)
        .expect("actual A read");
    let mut first_changes = first.subscribe();
    let bytes = harness
        .peer
        .next_node_frame(deadline())
        .await
        .expect("wire A")
        .expect("original A RPC");
    let doc_a = super::super::raw_json::RawDocument::parse(bytes).expect("A JSON");
    let rpc_a = doc_a
        .root()
        .object()
        .expect("A object")
        .get("id")
        .expect("unique decoded field")
        .expect("actual A ID");
    let second = harness
        .module
        .native_reads
        .request(sibling.key(), ReadReason::FirstOpen)
        .expect("actual B read");
    let mut second_changes = second.subscribe();
    let bytes = harness
        .peer
        .next_node_frame(deadline())
        .await
        .expect("wire B")
        .expect("original B RPC");
    let doc_b = super::super::raw_json::RawDocument::parse(bytes).expect("B JSON");
    let rpc_b = doc_b
        .root()
        .object()
        .expect("B object")
        .get("id")
        .expect("unique decoded field")
        .expect("actual B ID");
    assert_ne!(rpc_a.fragment(), rpc_b.fragment());
    let mut fault = harness
        .module
        .test_controller()
        .fail_next(
            ViewFalliblePoint::CaptureAppendReserve,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("next actual B capture only");
    harness.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sibling-session","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await;
    fault
        .triggered()
        .await
        .expect("actual B window/cell missing frame recorded before try_reserve");
    harness.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await;
    for rpc in [rpc_b, rpc_a] {
        let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
        response.extend_from_slice(rpc.fragment().as_bytes());
        response.extend_from_slice(b",\"result\":{}}");
        harness.send(&response).await;
    }
    timeout_at(deadline(),async {loop {
        let signal=harness.module.views().core.changed.clone();let wake=signal.notified();tokio::pin!(wake);wake.as_mut().enable();
        let current=harness.module.views().core.target(harness.target.key()).expect("actual A target");let mode=current.snapshot.read().expect("A snapshot").descriptor.read_mode();
        if harness.module.native_reads.status(&first).read_mode()==ReadMode::ReplayComplete&&harness.module.native_reads.status(&second).read_mode()==ReadMode::ReplayIncomplete&&mode==ReadMode::ReplayComplete {break;}
        tokio::select!{()=wake=>{},result=first_changes.changed()=>{result.expect("A metadata");},result=second_changes.changed()=>{result.expect("B metadata");}}
    }}).await.expect("A's own exact accepted cell remains complete while B preserves its real gap");
    let read = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("A H is still actually readable");
    assert_eq!(read.descriptor().read_mode(), ReadMode::ReplayComplete);
    assert!(
        harness
            .module
            .views()
            .core
            .probe
            .first_missing_sequence()
            .is_some()
    );
    drop(read);
    harness.close().await;
}

#[allow(
    clippy::too_many_lines,
    reason = "One real read window, controlled missing receipt and successor show exact old/new coverage without synthetic Received"
)]
async fn actual_gap_then_fresh_window(miss_response: bool) {
    use crate::application::shared_views::{budget::ReadReason, read_control::NativeReadControl};
    let mut harness = Harness::start().await;
    let original=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"old","status":"completed"}}}"#;
    let mut cut = harness
        .module
        .test_controller()
        .observe_once(
            ViewHookPoint::AfterWorkerCutProcessed,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("real old raw");
    harness.send(original).await;
    cut.arrived().await.expect("processed old raw");
    let retained = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("actual retained H");
    let h = retained.descriptor();
    let first = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::FirstOpen)
        .expect("original read");
    let mut changes = first.subscribe();
    let bytes = harness
        .peer
        .next_node_frame(deadline())
        .await
        .expect("wire")
        .expect("original request");
    let document = super::super::raw_json::RawDocument::parse(bytes).expect("request");
    let rpc = document
        .root()
        .object()
        .expect("object")
        .get("id")
        .expect("unique decoded field")
        .expect("actual typed RPC");
    let mut fault = if miss_response {
        None
    } else {
        Some(
            harness
                .module
                .test_controller()
                .fail_next(
                    ViewFalliblePoint::CaptureAppendReserve,
                    HookSelector::Source(harness.module.scope()),
                )
                .expect("actual next capture"),
        )
    };
    let mut state_cut = harness
        .module
        .test_controller()
        .observe_once(
            ViewHookPoint::AfterWorkerCutProcessed,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("actual worker cut");
    harness.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await;
    if let Some(fault) = fault.as_mut() {
        fault
            .triggered()
            .await
            .expect("actual own-window raw capture failure");
    }
    if miss_response {
        state_cut
            .arrived()
            .await
            .expect("the original state is captured and processed before failing response capture");
        fault = Some(
            harness
                .module
                .test_controller()
                .fail_next(
                    ViewFalliblePoint::CaptureAppendReserve,
                    HookSelector::Source(harness.module.scope()),
                )
                .expect("next actual response capture"),
        );
    }
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    response.extend_from_slice(rpc.fragment().as_bytes());
    response.extend_from_slice(b",\"result\":{}}");
    harness.send(&response).await;
    if miss_response {
        fault
            .as_mut()
            .expect("armed")
            .triggered()
            .await
            .expect("actual response capture failure");
    }
    timeout_at(deadline(),async {loop {
        let signal=harness.module.views().core.changed.clone();let wake=signal.notified();tokio::pin!(wake);wake.as_mut().enable();
        let current=harness.module.views().core.target(harness.target.key()).expect("retained old principal");let snapshot=current.snapshot.read().expect("snapshot").clone();
        if harness.module.native_reads.status(&first).read_mode()==ReadMode::ReplayIncomplete && snapshot.segments.publications.len()==2 {
            assert_eq!(snapshot.descriptor.view_epoch(),h.view_epoch(),"a gapped candidate cannot replace the old H");
            assert!(snapshot.segments.metadata(1,snapshot.descriptor.revision().as_u64()).expect("failed segment").completion.is_none(),"a gapped prefix never exposes ReplayComplete");
            break;
        }
        tokio::select!{()=wake=>{},result=changes.changed()=>{result.expect("original metadata");}}
    }}).await.expect("own-window gap reaches both actual request and old-view failure bundle");
    assert_eq!(retained.descriptor().revision(), h.revision());
    assert_eq!(
        retained.descriptor().freshness(),
        crate::domain::shared_view::Freshness::Stale
    );
    assert!(
        !harness.module.core.connection.is_closed(),
        "display capture failure cannot stop the actual protocol reader or native work"
    );
    assert!(
        harness
            .module
            .views()
            .core
            .probe
            .first_missing_sequence()
            .is_some()
    );
    let successor = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::FirstOpen)
        .expect("new actual operation");
    let mut changes = successor.subscribe();
    harness.complete_replay().await;
    timeout_at(deadline(),async {loop {
        let signal=harness.module.views().core.changed.clone();let wake=signal.notified();tokio::pin!(wake);wake.as_mut().enable();
        let current=harness.module.views().core.target(harness.target.key()).expect("new actual current");let snapshot=current.snapshot.read().expect("snapshot").clone();
        if harness.module.native_reads.status(&successor).read_mode()==ReadMode::ReplayComplete && snapshot.descriptor.read_mode()==ReadMode::ReplayComplete {
            assert_ne!(snapshot.descriptor.view_epoch(),h.view_epoch());assert_eq!(snapshot.segments.publications.len(),1);
            assert_eq!(snapshot.projection.records.len(),1,"new H contains only this actual unique window's state raw");
            break;
        }
        tokio::select!{()=wake=>{},result=changes.changed()=>{result.expect("original successor metadata");}}
    }}).await.expect("an older source gap cannot poison a genuinely fresh successful replay");
    assert!(
        harness
            .module
            .views()
            .core
            .probe
            .first_missing_sequence()
            .is_some(),
        "the old source diagnostic is preserved rather than reset"
    );
    let readable = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("new completed H remains readable despite an older source gap");
    assert_eq!(readable.descriptor().read_mode(), ReadMode::ReplayComplete);
    assert_eq!(
        harness.module.native_reads.status(&first).read_mode(),
        ReadMode::ReplayIncomplete
    );
    assert_eq!(harness.module.policy.calls.load(Ordering::Acquire), 2);
    drop(readable);
    drop(retained);
    harness.close().await;
}
#[tokio::test]
async fn shared_view_actual_state_capture_gap_keeps_old_h_and_fresh_exact_window_recovers() {
    actual_gap_then_fresh_window(false).await;
}
#[tokio::test]
async fn shared_view_actual_response_capture_gap_keeps_protocol_live_and_fresh_exact_window_recovers()
 {
    actual_gap_then_fresh_window(true).await;
}

#[tokio::test]
async fn shared_view_actual_failure_marker_allocation_uses_original_fifo_fallback_and_next_read_is_not_busy()
 {
    use crate::application::shared_views::{budget::ReadReason, read_control::NativeReadControl};
    let mut harness = Harness::start().await;
    let first = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::FirstOpen)
        .expect("first actual op");
    let mut changes = first.subscribe();
    let bytes = harness
        .peer
        .next_node_frame(deadline())
        .await
        .expect("wire")
        .expect("actual request");
    let document = super::super::raw_json::RawDocument::parse(bytes).expect("request");
    let rpc = document
        .root()
        .object()
        .expect("object")
        .get("id")
        .expect("unique decoded field")
        .expect("typed RPC");
    let mut fault = harness
        .module
        .test_controller()
        .fail_next(
            ViewFalliblePoint::RouteMarkerReserve,
            HookSelector::Target(harness.target.key().clone()),
        )
        .expect("one actual failure-marker append");
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    response.extend_from_slice(rpc.fragment().as_bytes());
    response.extend_from_slice(br#","error":{"code":-32000,"message":"self-owned fixture"}}"#);
    harness.send(&response).await;
    fault
        .triggered()
        .await
        .expect("real failure marker allocation path");
    timeout_at(deadline(),async {loop {
        let signal=harness.module.views().core.changed.clone();let wake=signal.notified();tokio::pin!(wake);wake.as_mut().enable();
        let current=harness.module.views().core.target(harness.target.key()).expect("actual current");let snapshot=current.snapshot.read().expect("snapshot").clone();
        if snapshot.segments.publications.len()==2&&harness.module.native_reads.status(&first).read_mode()==ReadMode::ReplayIncomplete {break;}
        tokio::select!{()=wake=>{},result=changes.changed()=>{result.expect("real read metadata");}}
    }}).await.expect("the preallocated original marker terminates the matching candidate");
    assert_eq!(
        harness.module.views().core.probe.failure(),
        None,
        "a marker allocation failure is not a raw/native gap"
    );
    let _next = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::FirstOpen)
        .expect("successor after actual terminal");
    harness.complete_replay().await;
    harness.wait_complete().await;
    assert_eq!(harness.module.policy.calls.load(Ordering::Acquire), 2);
    harness.close().await;
}

#[tokio::test]
async fn shared_view_real_response_deadline_is_observed_from_original_acp_owner_and_cleanup_keeps_error()
 {
    use crate::application::shared_views::read_control::NativeReadControl;
    let mut harness = Harness::start().await;
    let request = harness
        .module
        .replay(
            &harness.target,
            setup(Version::V2, "C:\\fixture"),
            NativeReadOptions {
                write_deadline: deadline(),
                response_wait: ReadResponseWait::At(Instant::now() + Duration::from_millis(75)),
            },
        )
        .await
        .expect("original explicit fixture IO options");
    let mut metadata = request.subscribe();
    let bytes = harness
        .peer
        .next_node_frame(deadline())
        .await
        .expect("original write")
        .expect("actual RPC reached peer");
    assert!(!bytes.is_empty());
    timeout_at(deadline(), async {
        loop {
            if harness.module.native_reads.status(&request).read_mode()
                == ReadMode::ReplayIncomplete
            {
                break;
            }
            metadata
                .changed()
                .await
                .expect("original operation updates");
        }
    })
    .await
    .expect("actual ACP deadline/terminal reaches the owned read");
    assert!(
        harness.module.core.connection.is_closed(),
        "the bridge cannot manufacture a deadline while the original ACP owner remains live"
    );
    assert_eq!(
        harness.module.policy.calls.load(Ordering::Acquire),
        0,
        "explicit fixture operation does not impersonate service-provider sampling"
    );
    let report = harness.module.shutdown(deadline()).await;
    assert!(
        report.io_joined() && report.pump_joined() && report.views_joined(),
        "original handles really join despite the protocol failure"
    );
    let owner = harness.owner.shutdown(deadline()).await;
    assert_eq!(owner.pending_tasks(), 0);
    assert_eq!(owner.pending_resources(), 0);
    assert_eq!(
        owner.failure(),
        Some(crate::shared_view_test_support::FixtureError::Io),
        "actual failed pump result is retained rather than counted as success"
    );
    harness.auth.close().await.expect("actual Node/SQL join and first physical area cleanup after original module resources closed");
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "The same real RPC and successor prove pinned joins, unchanged deadlines, ExplicitRefresh policy sampling and owner cleanup"
)]
async fn shared_view_real_inflight_refreshes_join_original_and_schedule_one_fresh_explicit_successor()
 {
    use crate::application::shared_views::{budget::ReadReason, read_control::NativeReadControl};
    let mut harness = Harness::start().await;
    let before = harness.module.core.enqueues.load(Ordering::Acquire);
    let first = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::FirstOpen)
        .expect("first operation");
    let first_frame = harness
        .peer
        .next_node_frame(deadline())
        .await
        .expect("first bytes")
        .expect("actual request");
    let first_document = super::super::raw_json::RawDocument::parse(first_frame).expect("request");
    let first_rpc = first_document
        .root()
        .object()
        .expect("object")
        .get("id")
        .expect("unique decoded field")
        .expect("typed RPC");
    let first_epoch = first.read_epoch().expect("same original bound read");
    let first_options = harness
        .module
        .native_reads
        .status(&first)
        .selected_options()
        .expect("original selected policy");
    let joined = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::ExplicitRefresh)
        .expect("inflight join");
    let also_joined = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::ExplicitRefresh)
        .expect("second inflight join");
    assert_eq!(joined.read_epoch(), Some(first_epoch));
    assert_eq!(also_joined.read_epoch(), Some(first_epoch));
    assert_ne!(first.request_id(), joined.request_id());
    assert_ne!(joined.request_id(), also_joined.request_id());
    assert_eq!(harness.module.policy.calls.load(Ordering::Acquire), 1);
    assert_eq!(
        harness
            .module
            .native_reads
            .status(&joined)
            .selected_options()
            .expect("same policy")
            .write_deadline(),
        first_options.write_deadline(),
        "join cannot reset an in-flight absolute deadline"
    );
    harness.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await;
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    response.extend_from_slice(first_rpc.fragment().as_bytes());
    response.extend_from_slice(b",\"result\":{}}");
    harness.send(&response).await;
    let second_frame = harness
        .peer
        .next_node_frame(deadline())
        .await
        .expect("automatic desired successor")
        .expect("exactly one new actual request");
    let second_document =
        super::super::raw_json::RawDocument::parse(second_frame).expect("request");
    let second_rpc = second_document
        .root()
        .object()
        .expect("object")
        .get("id")
        .expect("unique decoded field")
        .expect("new typed RPC");
    assert_ne!(first_rpc.fragment(), second_rpc.fragment());
    assert_eq!(
        harness.module.core.enqueues.load(Ordering::Acquire),
        before + 2
    );
    assert_eq!(harness.module.policy.calls.load(Ordering::Acquire), 2);
    assert_eq!(
        harness.module.policy.last_reason.load(Ordering::Acquire),
        1,
        "the actual fresh provider receives ExplicitRefresh rather than a fabricated FirstOpen"
    );
    assert_eq!(
        harness.module.native_reads.status(&first).read_epoch(),
        Some(first_epoch)
    );
    assert_eq!(
        harness.module.native_reads.status(&joined).read_mode(),
        ReadMode::ReplayComplete,
        "accepted requests remain pinned to their first original operation"
    );
    harness.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await;
    let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    response.extend_from_slice(second_rpc.fragment().as_bytes());
    response.extend_from_slice(b",\"result\":{}}");
    harness.send(&response).await;
    harness.wait_complete().await;
    assert_eq!(
        harness.module.core.enqueues.load(Ordering::Acquire),
        before + 2
    );
    assert_eq!(harness.module.policy.calls.load(Ordering::Acquire), 2);
    harness.close().await;
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "The same authenticated peer proves a genuinely empty failed read, independent live bytes and original-owner cleanup"
)]
async fn shared_view_real_remote_failure_without_cache_keeps_empty_failed_segment_and_live_raw() {
    use crate::application::shared_views::{budget::ReadReason, read_control::NativeReadControl};
    let mut harness = Harness::start().await;
    let request = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::FirstOpen)
        .expect("actual read");
    let bytes = harness
        .peer
        .next_node_frame(deadline())
        .await
        .expect("wire")
        .expect("original RPC");
    let document = super::super::raw_json::RawDocument::parse(bytes).expect("request");
    let rpc = document
        .root()
        .object()
        .expect("object")
        .get("id")
        .expect("unique decoded field")
        .expect("typed RPC");
    let mut error = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    error.extend_from_slice(rpc.fragment().as_bytes());
    error.extend_from_slice(br#","error":{"code":-32000,"message":"self-owned fixture"}}"#);
    harness.send(&error).await;
    timeout_at(deadline(), async {
        loop {
            let changed = harness.module.views().core.changed.clone();
            let wake = changed.notified();
            tokio::pin!(wake);
            wake.as_mut().enable();
            let current = harness
                .module
                .views()
                .core
                .target(harness.target.key())
                .expect("actual target");
            let snapshot = current.snapshot.read().expect("snapshot").clone();
            if snapshot.segments.publications.len() == 2
                && snapshot.descriptor.read_mode() == ReadMode::ReplayIncomplete
            {
                assert_eq!(
                    snapshot.projection.records.len(),
                    0,
                    "no prior cache and no replay raw must remain genuinely empty"
                );
                let failed = snapshot
                    .segments
                    .metadata(1, snapshot.descriptor.revision().as_u64())
                    .expect("real failed segment");
                assert!(
                    failed.begin.is_none()
                        && failed.through.is_none()
                        && failed.completion.is_none()
                );
                assert!(matches!(
                    failed.origin,
                    crate::application::shared_views::segments::SegmentOrigin::FailedReplay
                ));
                break;
            }
            wake.await;
        }
    })
    .await
    .expect("actual empty failure publication");
    let original=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"after empty failure"},"unknown":123456789012345678901234567890}}}"#;
    let mut cut = harness
        .module
        .test_controller()
        .observe_once(
            ViewHookPoint::AfterWorkerCutProcessed,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("actual worker");
    harness.send(original).await;
    cut.arrived().await.expect("live processed");
    let read = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("existing bundle has actual current auth");
    let stream = read.stream();
    let (node, mut peer) = tokio::io::duplex(128);
    let authority = harness.auth.authority();
    let (mut writer, mut credits) = stream
        .bind_duplex(
            node,
            &authority
                .client_binding(&harness.client)
                .expect("same binding"),
        )
        .expect("real endpoints");
    peer.write_all(
        &TransferHeader::credit(stream.id(), 0)
            .encode(&[])
            .expect("wire"),
    )
    .await
    .expect("initial actual credit");
    credits
        .read_credit(
            authority.as_ref(),
            &harness.client,
            harness.target.key(),
            deadline(),
        )
        .await
        .expect("actual auth and credit");
    let chunk = harness
        .prepare_raw_after_segment_status(&read, &mut writer, &mut credits, &mut peer)
        .await;
    let segment = chunk.address().segment_id();
    let packet = chunk.packet_sequence();
    let send = writer.write_chunk(
        chunk.into_local(),
        authority.as_ref(),
        &harness.client,
        harness.target.key(),
        deadline(),
    );
    let receive = async {
        let mut header = [0; HEADER_BYTES];
        peer.read_exact(&mut header)
            .await
            .expect("actual raw header");
        let size = u32::from_be_bytes(header[64..68].try_into().expect("fixed length")) as usize;
        let mut bytes = header.to_vec();
        bytes.resize(HEADER_BYTES + size, 0);
        peer.read_exact(&mut bytes[HEADER_BYTES..])
            .await
            .expect("actual raw bytes");
        assert_eq!(TransferHeader::decode(&bytes).expect("codec").1, original);
    };
    let (flushed, ()) = tokio::join!(send, receive);
    flushed.expect("actual flush");
    peer.write_all(
        &TransferHeader::credit(stream.id(), packet)
            .encode(&[])
            .expect("wire"),
    )
    .await
    .expect("actual raw credit");
    credits
        .read_credit(
            authority.as_ref(),
            &harness.client,
            harness.target.key(),
            deadline(),
        )
        .await
        .expect("actual raw commit");
    let inspected = harness
        .module
        .probe()
        .target(harness.target.key())
        .expect("actual target");
    assert_eq!(inspected.anchor_count(), 1);
    assert_eq!(inspected.anchors()[0].segment_id(), segment);
    assert_eq!(
        harness.module.native_reads.status(&request).read_mode(),
        ReadMode::ReplayIncomplete
    );
    assert_eq!(
        harness.module.policy.calls.load(Ordering::Acquire),
        1,
        "empty/failure/live display adds no operation"
    );
    drop(read);
    harness.close().await;
}
#[tokio::test]
async fn shared_view_real_route_marker_allocation_failure_is_not_a_raw_gap_and_next_op_recovers() {
    use crate::application::shared_views::{budget::ReadReason, read_control::NativeReadControl};
    let mut harness = Harness::start().await;
    let before = harness.module.core.enqueues.load(Ordering::Acquire);
    let mut fault = harness
        .module
        .test_controller()
        .fail_next(
            ViewFalliblePoint::RouteMarkerReserve,
            HookSelector::Target(harness.target.key().clone()),
        )
        .expect("one actual reserve path");
    let failed = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::FirstOpen)
        .expect("owned request");
    fault
        .triggered()
        .await
        .expect("actual route marker try_reserve path");
    timeout_at(deadline(), async {
        loop {
            let changed = harness.module.views().core.changed.clone();
            let wake = changed.notified();
            tokio::pin!(wake);
            wake.as_mut().enable();
            let current = harness
                .module
                .views()
                .core
                .target(harness.target.key())
                .expect("retained cache");
            let descriptor = current.snapshot.read().expect("snapshot").descriptor;
            if descriptor.failure() == Some(ViewError::AllocationFailure)
                && harness.module.native_reads.status(&failed).read_mode()
                    == ReadMode::ReplayIncomplete
            {
                break;
            }
            tokio::select! {()=wake=>{},()=tokio::task::yield_now()=>{}}
        }
    })
    .await
    .expect("display-only fault reaches retained current and request");
    assert_eq!(
        harness.module.core.enqueues.load(Ordering::Acquire),
        before,
        "the original never-enqueued reservation is cancelled"
    );
    assert_eq!(
        harness.module.views().core.probe.failure(),
        None,
        "metadata allocation is not a native/raw capture gap"
    );
    assert_eq!(
        harness.module.views().core.probe.first_missing_sequence(),
        None
    );
    let read = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("existing empty/stale view remains readable");
    assert_eq!(
        read.descriptor().freshness(),
        crate::domain::shared_view::Freshness::Stale
    );
    drop(read);
    let _next = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::FirstOpen)
        .expect("fresh successor after real cancellation");
    harness.complete_replay().await;
    harness.wait_complete().await;
    assert_eq!(
        harness.module.core.enqueues.load(Ordering::Acquire),
        before + 1
    );
    assert_eq!(harness.module.policy.calls.load(Ordering::Acquire), 2);
    assert_eq!(
        harness.module.native_reads.status(&failed).read_mode(),
        ReadMode::ReplayIncomplete
    );
    harness.close().await;
}
#[tokio::test]
async fn shared_view_real_capture_fault_wakes_idle_wait_without_later_output() {
    let mut harness = Harness::start().await;
    let read = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("actual current");
    let stream = read.stream();
    let (node, mut peer) = tokio::io::duplex(128);
    let authority = harness.auth.authority();
    let (_writer, mut credits) = stream
        .bind_duplex(
            node,
            &authority.client_binding(&harness.client).expect("binding"),
        )
        .expect("real transport");
    peer.write_all(
        &TransferHeader::credit(stream.id(), 0)
            .encode(&[])
            .expect("wire"),
    )
    .await
    .expect("real initial credit");
    credits
        .read_credit(
            authority.as_ref(),
            &harness.client,
            harness.target.key(),
            deadline(),
        )
        .await
        .expect("true credit");
    let before = authority.test_registered_waits();
    let mut fault = harness
        .module
        .test_controller()
        .fail_next(
            ViewFalliblePoint::CaptureAppendReserve,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("one new raw append path");
    let reads = Arc::clone(&harness.module.reads);
    let client = harness.client.owned_task_ref();
    let waiting = reads.wait_change(&client, &read);
    let trigger = async {
        timeout_at(deadline(), async {
            while authority.test_registered_waits() <= before {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("real post-SQL idle registration");
        harness.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"controlled failed raw append"}}}}"#).await;
        fault.triggered().await.expect("actual append failure");
    };
    let (result, ()) = timeout_at(deadline(), async { tokio::join!(waiting, trigger) })
        .await
        .expect("idle waiter is woken without any later native output");
    let descriptor = result.expect("typed stale view descriptor");
    assert_eq!(descriptor.failure(), Some(ViewError::AllocationFailure));
    assert_eq!(
        descriptor.freshness(),
        crate::domain::shared_view::Freshness::Stale
    );
    assert!(
        harness
            .module
            .views()
            .core
            .probe
            .first_missing_sequence()
            .is_some()
    );
    drop(read);
    harness.close().await;
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "One real source, failed operation, authorized byte peer and successor preserve their causal ownership through cleanup"
)]
async fn shared_view_real_remote_failure_keeps_old_h_failed_and_live_segments_then_strict_new_replay()
 {
    use crate::application::shared_views::{
        budget::ReadReason, read_control::NativeReadControl, wire::RecordKind,
    };
    use crate::domain::shared_view::{Freshness, JoinState};
    let mut harness = Harness::start().await;
    let old=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"shared-id","status":"pending","rawInput":123456789012345678901234567890123456789}}}"#;
    let failed=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"shared-id","status":"failed","rawOutput":{"unknown":1e999999}}}}"#;
    let live=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"shared-id","status":"completed","title":"live independent domain"}}}"#;
    let mut cut = harness
        .module
        .test_controller()
        .observe_once(
            ViewHookPoint::AfterWorkerCutProcessed,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("actual worker observation");
    harness.send(old).await;
    cut.arrived().await.expect("old actual raw processed");
    let retained = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("authorized old H");
    let h = retained.descriptor();
    let epoch = h.view_epoch();
    let before = harness.module.core.enqueues.load(Ordering::Acquire);
    let request = harness
        .module
        .native_reads
        .request(harness.target.key(), ReadReason::ExplicitRefresh)
        .expect("one original owned read");
    let rpc_frame = harness
        .peer
        .next_node_frame(deadline())
        .await
        .expect("real request IO")
        .expect("original replay");
    let document = super::super::raw_json::RawDocument::parse(rpc_frame).expect("request JSON");
    let rpc = document
        .root()
        .object()
        .expect("object")
        .get("id")
        .expect("unique decoded field")
        .expect("original typed RPC");
    harness.send(failed).await;
    let mut error = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    error.extend_from_slice(rpc.fragment().as_bytes());
    error.extend_from_slice(br#","error":{"code":-32000,"message":"self-owned fixture"}}"#);
    super::super::RawMessage::parse(error.clone())
        .expect("the peer's error is valid original JSON-RPC");
    harness.send(&error).await;
    timeout_at(deadline(), async {
        loop {
            let changed = harness.module.views().core.changed.clone();
            let wake = changed.notified();
            tokio::pin!(wake);
            wake.as_mut().enable();
            let current = harness
                .module
                .views()
                .core
                .target(harness.target.key())
                .expect("actual target");
            let snapshot = current.snapshot.read().expect("snapshot").clone();
            let state = harness.module.native_reads.status(&request);
            if snapshot.segments.publications.len() == 2
                && state.read_mode() == ReadMode::ReplayIncomplete
                && state.transport()
                    == crate::application::shared_views::read_control::ReadTransportState::Written
            {
                break;
            }
            wake.await;
        }
    })
    .await
    .expect("actual failure FIFO and original ticket terminal");
    assert_eq!(retained.descriptor().view_epoch(), epoch);
    assert_eq!(retained.descriptor().revision(), h.revision());
    assert_eq!(retained.descriptor().freshness(), Freshness::Stale);
    let mut cut = harness
        .module
        .test_controller()
        .observe_once(
            ViewHookPoint::AfterWorkerCutProcessed,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("actual live worker observation");
    harness.send(live).await;
    cut.arrived().await.expect("live actual raw processed");
    let inspected = harness
        .module
        .probe()
        .target(harness.target.key())
        .expect("same authenticated-source target metadata");
    assert_eq!(
        inspected.anchor_count(),
        3,
        "same native tool ID never joins old/failed/live identity domains"
    );
    let segment_ids: std::collections::HashSet<_> = inspected
        .anchors()
        .iter()
        .map(crate::application::shared_views::probe::AnchorMetadata::segment_id)
        .collect();
    assert_eq!(segment_ids.len(), 3);
    let mut transfer = harness
        .module
        .open_transfer(&harness.client, &harness.target, ReadReason::FirstOpen)
        .await
        .expect("authorized failed bundle bytes");
    let successor = harness
        .peer
        .next_node_frame(deadline())
        .await
        .expect("real next operation")
        .expect("next replay after original safe terminal");
    let successor = super::super::raw_json::RawDocument::parse(successor).expect("request");
    let successor_rpc = successor
        .root()
        .object()
        .expect("object")
        .get("id")
        .expect("unique decoded field")
        .expect("new original RPC");
    assert_ne!(successor_rpc.fragment(), rpc.fragment());
    assert_eq!(
        harness.module.core.enqueues.load(Ordering::Acquire),
        before + 2
    );
    assert_eq!(
        harness.module.policy.calls.load(Ordering::Acquire),
        2,
        "one fresh provider per actual operation"
    );
    let mut packet = 0_u64;
    let mut raw = Vec::new();
    let mut raw_segments = Vec::new();
    let mut anchor_ids = std::collections::HashSet::new();
    let mut statuses = 0;
    loop {
        let record = timeout_at(deadline(), transfer.next_record())
            .await
            .expect("real authorized read")
            .expect("actual bytes");
        packet +=
            u64::try_from(record.bytes().len().div_ceil(16 * 1024).max(1)).expect("packet count");
        let kind = record.address().kind();
        let end = kind == RecordKind::SnapshotEnd;
        match kind {
            RecordKind::OriginalRaw => {
                raw.push(record.bytes().to_vec());
                raw_segments.push(record.address().segment_id());
            }
            RecordKind::Status => {
                statuses += 1;
                let metadata: serde_json::Value =
                    serde_json::from_slice(record.bytes()).expect("platform-only segment Status");
                assert_eq!(metadata["type"], "view_segment");
                assert_eq!(metadata["relation"], "connection_observation_only");
                assert_eq!(
                    metadata["segmentId"],
                    record.address().segment_id().encode()
                );
                assert!(!String::from_utf8_lossy(record.bytes()).contains("control-session"));
                assert!(!String::from_utf8_lossy(record.bytes()).contains("shared-id"));
            }
            RecordKind::ItemProjection => {
                let metadata: serde_json::Value =
                    serde_json::from_slice(record.bytes()).expect("platform projection only");
                assert_eq!(
                    metadata["segmentId"],
                    record.address().segment_id().encode()
                );
                if metadata["type"] == "item_projection" {
                    anchor_ids.insert(
                        metadata["anchorId"]
                            .as_str()
                            .expect("platform anchor")
                            .to_owned(),
                    );
                }
            }
            RecordKind::SnapshotEnd => {}
        }
        transfer
            .credit(packet)
            .await
            .expect("each actual flush and matching peer credit");
        if end {
            break;
        }
    }
    assert!(
        statuses >= 6,
        "each immutable H segment has real start and end Status"
    );
    assert_eq!(
        raw,
        vec![old.to_vec(), failed.to_vec(), live.to_vec()],
        "original unknown numeric lexemes and order survive authorized peer bytes"
    );
    assert_eq!(
        raw_segments
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );
    assert_eq!(
        anchor_ids.len(),
        3,
        "bundle platform anchors do not collide"
    );
    assert_eq!(
        harness
            .module
            .probe()
            .target(harness.target.key())
            .expect("old bundle retained")
            .descriptor()
            .join(),
        JoinState::UnassociatedSegments
    );
    let receipt = transfer.detach().await.expect("real display release");
    assert_eq!(receipt.last_committed_packet(), packet);
    let new=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"shared-id","status":"completed","title":"new replay only"}}}"#;
    harness.send(new).await;
    harness.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await;
    let mut complete = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    complete.extend_from_slice(successor_rpc.fragment().as_bytes());
    complete.extend_from_slice(b",\"result\":{}}");
    harness.send(&complete).await;
    harness.wait_complete().await;
    let current = harness
        .module
        .views()
        .core
        .target(harness.target.key())
        .expect("new current");
    let snapshot = current.snapshot.read().expect("snapshot").clone();
    assert_eq!(
        snapshot.projection.records.len(),
        2,
        "only this unique original replay window belongs to new complete H"
    );
    assert_eq!(snapshot.segments.publications.len(), 1);
    assert_ne!(snapshot.descriptor.view_epoch(), epoch);
    assert_eq!(
        harness.module.native_reads.status(&request).read_mode(),
        ReadMode::ReplayIncomplete,
        "original failed request remains pinned"
    );
    assert_eq!(
        harness.module.core.enqueues.load(Ordering::Acquire),
        before + 2
    );
    assert_eq!(harness.module.policy.calls.load(Ordering::Acquire), 2);
    let _ = harness.module.reads.release(retained);
    drop(snapshot);
    drop(current);
    drop(inspected);
    harness.close().await;
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
impl Harness {
    async fn prepare_raw_after_segment_status(
        &self,
        read: &crate::application::shared_views::reader::OpenedView,
        writer: &mut crate::application::shared_views::local_transfer::LocalChunkWriter,
        credits: &mut crate::application::shared_views::local_transfer::LocalCreditReader,
        peer: &mut tokio::io::DuplexStream,
    ) -> crate::application::shared_views::reader::PreparedViewChunk {
        loop {
            let PrepareNext::Chunk(chunk) = self
                .module
                .reads
                .next_snapshot(&self.client, read)
                .await
                .expect("actual qualified preparation")
            else {
                panic!("real permit");
            };
            if chunk.address().kind()
                == crate::application::shared_views::wire::RecordKind::OriginalRaw
            {
                return chunk;
            }
            assert_eq!(
                chunk.address().kind(),
                crate::application::shared_views::wire::RecordKind::Status
            );
            let packet = chunk.packet_sequence();
            let authority = self.auth.authority();
            let before = read.snapshot_cursor();
            let send = writer.write_chunk(
                chunk.into_local(),
                authority.as_ref(),
                &self.client,
                self.target.key(),
                deadline(),
            );
            let receive = async {
                let mut header = [0; HEADER_BYTES];
                peer.read_exact(&mut header)
                    .await
                    .expect("real segment header");
                let size =
                    u32::from_be_bytes(header[64..68].try_into().expect("fixed payload")) as usize;
                let mut bytes = header.to_vec();
                bytes.resize(HEADER_BYTES + size, 0);
                peer.read_exact(&mut bytes[HEADER_BYTES..])
                    .await
                    .expect("real segment payload");
                let (actual, body) = TransferHeader::decode(&bytes).expect("real codec");
                assert_eq!(actual.packet(), packet);
                let status: serde_json::Value =
                    serde_json::from_slice(body).expect("platform-only metadata");
                assert_eq!(status["type"], "view_segment");
            };
            let (flushed, ()) = tokio::join!(send, receive);
            flushed.expect("actual metadata flush");
            assert_eq!(
                read.snapshot_cursor(),
                before,
                "metadata flush does not commit raw cursor"
            );
            peer.write_all(
                &TransferHeader::credit(read.stream().id(), packet)
                    .encode(&[])
                    .expect("wire credit"),
            )
            .await
            .expect("actual peer credit");
            credits
                .read_credit(
                    authority.as_ref(),
                    &self.client,
                    self.target.key(),
                    deadline(),
                )
                .await
                .expect("current actor and original sent packet");
            assert_eq!(
                read.snapshot_cursor(),
                before,
                "metadata commit does not advance raw bytes"
            );
        }
    }
    async fn start() -> Self {
        let area = OwnedTestArea::create(&std::env::temp_dir(), "shared-view-control")
            .expect("owned child");
        let cert =
            rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("self TLS");
        area.install_tls(
            cert.cert.pem().as_bytes(),
            cert.signing_key.serialize_pem().as_bytes(),
        )
        .expect("create_new");
        let owner = FixtureTaskOwner::new(&area, tokio::runtime::Handle::current())
            .expect("owner before IO");
        let auth = Arc::new(
            LocalViewAuthFixture::start(area)
                .await
                .expect("actual Node"),
        );
        let signer = FixtureSigningClient::generate().expect("synthetic key");
        let client = auth.authenticate(&signer).await.expect("real pair/proof");
        let (wire, mut peer) =
            FixturePeerWire::from_duplex(&owner.handle(), tokio::io::duplex(128 * 1024))
                .unwrap_or_else(|_| panic!("original endpoints"));
        let driver = tokio::spawn(async move {
            super::tests::reply(&mut peer,br#"{"protocolVersion":2,"info":{"name":"fixture","version":"2"},"capabilities":{"session":{}}}"#).await;
            peer
        });
        let module = AcpViewModuleFixture::start(
            FixtureSourceConfig {
                source: NativeSourceId(EntityId::from_bytes([93; 16])),
                entry_alias: Arc::from("control"),
            },
            wire,
            Arc::clone(&auth),
        )
        .await
        .expect("real init");
        let mut peer = driver.await.expect("driver joined");
        let driver = tokio::spawn(async move {
            super::tests::reply(&mut peer, br#"{"sessionId":"control-session"}"#).await;
            peer
        });
        let target = module
            .create(setup(Version::V2, "C:\\fixture"))
            .await
            .expect("actual new SID");
        Self {
            auth,
            owner,
            module,
            target,
            peer: driver.await.expect("driver joined"),
            client,
        }
    }
    async fn send(&mut self, body: &[u8]) {
        self.peer
            .write_bytes(body, deadline())
            .await
            .expect("actual peer write");
        self.peer.write_bytes(b"\n", deadline()).await.expect("LF");
    }
    async fn complete_replay(&mut self) {
        let request = self
            .peer
            .next_node_frame(deadline())
            .await
            .expect("read IO")
            .expect("actual request");
        let document = super::super::raw_json::RawDocument::parse(request).expect("request");
        let id = document
            .root()
            .object()
            .expect("object")
            .get("id")
            .expect("unique decoded field")
            .expect("typed rpc");
        self.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await;
        let mut response = br#"{"jsonrpc":"2.0","id":"#.to_vec();
        response.extend_from_slice(id.fragment().as_bytes());
        response.extend_from_slice(b",\"result\":{}}");
        self.send(&response).await;
    }
    async fn wait_complete(&self) {
        let views = self.module.views();
        timeout_at(deadline(), async {
            loop {
                let changed = views.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let target = views.core.target(self.target.key()).expect("bound target");
                if target
                    .snapshot
                    .read()
                    .expect("snapshot")
                    .descriptor
                    .read_mode()
                    == ReadMode::ReplayComplete
                {
                    return;
                }
                changed.await;
            }
        })
        .await
        .expect("true completion publication");
    }
    async fn close(self) {
        let report = self.module.shutdown(deadline()).await;
        assert!(report.io_joined() && report.pump_joined() && report.views_joined());
        assert!(self.owner.shutdown(deadline()).await.all_joined());
        self.auth
            .close()
            .await
            .expect("real Node/SQL/owned area cleanup");
    }
}
#[tokio::test]
async fn shared_view_actual_controller_worker_pause_keeps_original_protocol_pump_and_ticket_progress()
 {
    let mut harness = Harness::start().await;
    let controller = harness.module.test_controller();
    assert!(matches!(
        controller.arm_pause(ViewHookPoint::WriterGateHeld, HookSelector::Any),
        Err(crate::shared_view_test_support::FixtureError::Unsupported)
    ));
    let mut gate = controller
        .arm_pause(
            ViewHookPoint::AfterCaptureEnqueue,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("same actual source");
    harness.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed"}}}"#).await;
    let observation = timeout_at(deadline(), gate.arrived())
        .await
        .expect("real worker point")
        .expect("arrived");
    assert_eq!(observation.scope(), Some(&harness.module.scope()));
    let sequence = observation.sequence().expect("original capture sequence");
    let views = harness.module.views();
    assert!(views.core.probe.retained_frontier() >= sequence);
    assert!(views.core.probe.processed_frontier() < sequence);
    let request = harness
        .module
        .replay(
            &harness.target,
            setup(Version::V2, "C:\\fixture"),
            NativeReadOptions {
                write_deadline: deadline(),
                response_wait: ReadResponseWait::Unbounded,
            },
        )
        .await
        .expect("real operation accepted");
    harness.complete_replay().await;
    timeout_at(deadline(), async {
        loop {
            let changed = harness.module.core.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let written = {
                let operations = harness.module.core.operations.lock().expect("operations");
                operations.iter().any(|operation| {
                    matches!(operation.kind, RequestKind::Replay(_))
                        && operation.ticket.as_ref().is_some_and(|ticket| {
                            matches!(
                                ticket.progress(),
                                WriteProgress::Finished(WriteReport::Written)
                            )
                        })
                        && matches!(
                            *operation.completed.borrow(),
                            Some(Ok(OperationResult::Replayed))
                        )
                })
            };
            if written {
                break;
            }
            changed.await;
        }
    })
    .await
    .expect("original protocol receiver and write owner continue while display worker is paused");
    assert!(request.read_epoch().is_some());
    gate.release();
    harness.wait_complete().await;
    harness.close().await;
}
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "Keeps actual fault, authenticated peer bytes, unchanged cursor, and physical cleanup in one owned scenario"
)]
async fn shared_view_actual_controlled_projection_and_chunk_faults_keep_raw_and_current_cursor() {
    let mut harness = Harness::start().await;
    let controller = harness.module.test_controller();
    let mut fault = controller
        .fail_next(
            ViewFalliblePoint::ProjectionMetadataReserve,
            HookSelector::Target(harness.target.key().clone()),
        )
        .expect("one named new metadata path");
    let original=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"t","rawInput":1234567890123456789012345678901234567890}}}"#;
    harness.send(original).await;
    let observed = timeout_at(deadline(), fault.triggered())
        .await
        .expect("actual controlled branch")
        .expect("fault observation");
    let views = harness.module.views();
    views
        .core
        .probe
        .wait_processed(observed.sequence().expect("actual Received"))
        .await
        .expect("raw processed independently of projection metadata");
    let read = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("same currently authenticated raw read");
    assert_eq!(
        read.descriptor().projection(),
        ProjectionState::ProjectionUnavailable
    );
    assert_eq!(
        read.descriptor().failure(),
        Some(ViewError::AllocationFailure)
    );
    assert_eq!(
        views.core.probe.failure(),
        None,
        "projection fault is not a raw/native capture gap"
    );
    let stream = read.stream();
    let (node, mut peer) = tokio::io::duplex(128);
    let authority = harness.auth.authority();
    let (mut writer, mut credits) = stream
        .bind_duplex(
            node,
            &authority.client_binding(&harness.client).expect("binding"),
        )
        .expect("real display endpoints");
    peer.write_all(
        &TransferHeader::credit(stream.id(), 0)
            .encode(&[])
            .expect("wire"),
    )
    .await
    .expect("real initial credit");
    credits
        .read_credit(
            authority.as_ref(),
            &harness.client,
            harness.target.key(),
            deadline(),
        )
        .await
        .expect("current actor");
    let mut chunk_fault = controller
        .fail_next(
            ViewFalliblePoint::ReadChunkReserve,
            HookSelector::Stream(read.stream_id()),
        )
        .expect("only the actual new copy");
    assert!(matches!(
        harness
            .module
            .reads
            .next_snapshot(&harness.client, &read)
            .await,
        Err(ViewError::AllocationFailure)
    ));
    chunk_fault
        .triggered()
        .await
        .expect("actual named copy failure");
    assert_eq!(
        stream.observation().expect("same stream").highest_committed,
        0
    );
    let chunk = harness
        .prepare_raw_after_segment_status(&read, &mut writer, &mut credits, &mut peer)
        .await;
    let raw_packet = chunk.packet_sequence();
    let send = writer.write_chunk(
        chunk.into_local(),
        authority.as_ref(),
        &harness.client,
        harness.target.key(),
        deadline(),
    );
    let receive = async {
        let mut header = [0; HEADER_BYTES];
        peer.read_exact(&mut header).await.expect("actual header");
        let size = u32::from_be_bytes(header[64..68].try_into().expect("fixed bytes")) as usize;
        let mut packet = header.to_vec();
        packet.resize(HEADER_BYTES + size, 0);
        peer.read_exact(&mut packet[HEADER_BYTES..])
            .await
            .expect("actual raw");
        let (_, body) = TransferHeader::decode(&packet).expect("same codec");
        assert_eq!(body, original);
    };
    let (flushed, ()) = tokio::join!(send, receive);
    flushed.expect("real flush");
    peer.write_all(
        &TransferHeader::credit(stream.id(), raw_packet)
            .encode(&[])
            .expect("credit"),
    )
    .await
    .expect("actual credit");
    assert_eq!(
        credits
            .read_credit(
                authority.as_ref(),
                &harness.client,
                harness.target.key(),
                deadline()
            )
            .await
            .expect("actual qualified commit"),
        LocalCreditResult::Committed
    );
    drop(read);
    harness.close().await;
}
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "The genuine foreign scope, real byte transport, pre-commit cursor and physical cleanup stay in one owned probe scenario"
)]
async fn shared_view_actual_readonly_probe_enumerates_slot_ids_and_cursor_moves_only_after_credit()
{
    let mut harness = Harness::start().await;
    let views = harness.module.views();
    views
        .core
        .probe
        .wait_barrier(
            views.core.probe.retained_frontier(),
            views.core.probe.route_version(),
        )
        .await
        .expect("initial true route");
    let mut observed = harness
        .module
        .test_controller()
        .observe_once(
            ViewHookPoint::AfterWorkerCutProcessed,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("one actual worker event");
    let original=br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"probe-tool","title":"same original anchor"}}}"#;
    harness.send(original).await;
    observed.arrived().await.expect("actual source processed");
    let probe = harness.module.probe();
    let inspected = probe.source(&harness.module.scope()).expect("same source");
    assert_eq!(inspected.scope(), &harness.module.scope());
    let (node, _peer) = tokio::io::duplex(128);
    let (reader, writer) = tokio::io::split(node);
    let (foreign, foreign_events, foreign_io) = Connection::spawn(reader, writer)
        .expect("another genuine Connection creates its own scope");
    assert!(matches!(
        probe.source(&foreign.scope()),
        Err(ViewError::InvalidScope)
    ));
    drop(foreign_events);
    assert!(foreign_io.shutdown(deadline()).await.joined);
    let target = probe
        .target(harness.target.key())
        .expect("actual bound key");
    assert_eq!(target.anchor_count(), 1);
    let actual_id = target.anchors()[0].anchor_id();
    assert_eq!(
        target
            .anchor(actual_id)
            .expect("original typed data id")
            .raw_records()
            .len(),
        1
    );
    let read = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("true current qualification");
    assert_eq!(
        read.state(),
        crate::application::shared_views::reader::ReadStreamState::AwaitInitialCredit
    );
    let before = read.snapshot_cursor();
    assert!(!before.exhausted());
    assert_eq!(before.byte_offset(), 0);
    let stream = read.stream();
    let (node, mut peer) = tokio::io::duplex(128);
    let authority = harness.auth.authority();
    let (mut writer, mut credits) = stream
        .bind_duplex(
            node,
            &authority.client_binding(&harness.client).expect("binding"),
        )
        .expect("same actual peer");
    peer.write_all(
        &TransferHeader::credit(stream.id(), 0)
            .encode(&[])
            .expect("wire"),
    )
    .await
    .expect("initial actual credit");
    credits
        .read_credit(
            authority.as_ref(),
            &harness.client,
            harness.target.key(),
            deadline(),
        )
        .await
        .expect("actual parse/current actor");
    let chunk = harness
        .prepare_raw_after_segment_status(&read, &mut writer, &mut credits, &mut peer)
        .await;
    let raw_packet = chunk.packet_sequence();
    assert_eq!(
        chunk
            .metadata()
            .source_range()
            .expect("original Received range")
            .first_sequence(),
        inspected.processed_frontier()
    );
    assert_eq!(
        read.snapshot_cursor(),
        before,
        "preparation is not cursor authority"
    );
    let send = writer.write_chunk(
        chunk.into_local(),
        authority.as_ref(),
        &harness.client,
        harness.target.key(),
        deadline(),
    );
    let receive = async {
        let mut header = [0; HEADER_BYTES];
        peer.read_exact(&mut header).await.expect("header");
        let size = u32::from_be_bytes(header[64..68].try_into().expect("fixed bytes")) as usize;
        let mut packet = header.to_vec();
        packet.resize(HEADER_BYTES + size, 0);
        peer.read_exact(&mut packet[HEADER_BYTES..])
            .await
            .expect("body");
        assert_eq!(
            TransferHeader::decode(&packet).expect("actual codec").1,
            original
        );
    };
    let (flushed, ()) = tokio::join!(send, receive);
    flushed.expect("actual flush");
    assert_eq!(
        read.snapshot_cursor(),
        before,
        "flush without matching peer credit is not a commit"
    );
    peer.write_all(
        &TransferHeader::credit(stream.id(), raw_packet)
            .encode(&[])
            .expect("wire"),
    )
    .await
    .expect("actual ack");
    credits
        .read_credit(
            authority.as_ref(),
            &harness.client,
            harness.target.key(),
            deadline(),
        )
        .await
        .expect("same real actor commit");
    assert_ne!(read.snapshot_cursor().next_record(), before.next_record());
    assert!(
        !read.snapshot_cursor().exhausted(),
        "projection and SnapshotEnd remain until their actual commits"
    );
    assert_eq!(
        probe
            .stream(read.stream_id())
            .expect("same registered weak stream slot")
            .highest_committed(),
        raw_packet
    );
    drop(read);
    assert_eq!(
        probe
            .target(harness.target.key())
            .expect("browser release keeps node cache")
            .anchor_count(),
        1
    );
    harness.close().await;
}
#[tokio::test]
async fn shared_view_actual_replay_completion_survives_source_cache_release_and_descriptor_turns_stale()
 {
    let mut harness = Harness::start().await;
    let request = harness
        .module
        .replay(
            &harness.target,
            setup(Version::V2, "C:\\fixture"),
            NativeReadOptions {
                write_deadline: deadline(),
                response_wait: ReadResponseWait::Unbounded,
            },
        )
        .await
        .expect("actual accepted Replay");
    harness.complete_replay().await;
    harness.wait_complete().await;
    let read = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("actual completed view H");
    assert_eq!(read.descriptor().read_mode(), ReadMode::ReplayComplete);
    harness.peer.close(deadline()).await;
    timeout_at(deadline(), async {
        loop {
            let changed = harness.module.core.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if harness.module.core.pump_joined.load(Ordering::Acquire) {
                break;
            }
            changed.await;
        }
    })
    .await
    .expect("true original reader exits");
    let mut status = request.subscribe();
    timeout_at(deadline(), async {
        loop {
            if status.borrow_and_update().read_mode() == ReadMode::ReplayComplete {
                break;
            }
            status.changed().await.expect("original read owner status");
        }
    })
    .await
    .expect("original receipt/raw-prefix completion remains observable without a cache");
    assert_eq!(
        read.descriptor().freshness(),
        crate::domain::shared_view::Freshness::Stale
    );
    assert_eq!(read.descriptor().read_mode(), ReadMode::ReplayComplete);
    assert!(
        matches!(
            harness
                .module
                .reads
                .rebind_same_read(&harness.client, &read)
                .await,
            Err(ViewError::SourceReleased)
        ),
        "a retained completed fact does not authorize a closed source"
    );
    drop(read);
    harness.close().await;
}
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep the actual terminal, paused original worker, zero-start send fence and real owner join in one causal scenario"
)]
async fn shared_view_actual_source_terminal_seals_prepared_send_before_view_worker_join() {
    let mut harness = Harness::start().await;
    let views = harness.module.views();
    views
        .core
        .probe
        .wait_barrier(
            views.core.probe.retained_frontier(),
            views.core.probe.route_version(),
        )
        .await
        .expect("true initial route");
    let controller = harness.module.test_controller();
    let mut processed = controller
        .observe_once(
            ViewHookPoint::AfterWorkerCutProcessed,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("actual worker event");
    harness.send(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"control-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"retained"}}}}"#).await;
    processed.arrived().await.expect("actual retained H");
    let read = harness
        .module
        .reads
        .open_current(&harness.client, harness.target.key())
        .await
        .expect("current actual client");
    let stream = read.stream();
    let (node, mut peer) = tokio::io::duplex(128);
    let authority = harness.auth.authority();
    let (mut writer, mut credits) = stream
        .bind_duplex(
            node,
            &authority
                .client_binding(&harness.client)
                .expect("same binding"),
        )
        .expect("actual endpoints");
    peer.write_all(
        &TransferHeader::credit(stream.id(), 0)
            .encode(&[])
            .expect("wire"),
    )
    .await
    .expect("initial credit");
    credits
        .read_credit(
            authority.as_ref(),
            &harness.client,
            harness.target.key(),
            deadline(),
        )
        .await
        .expect("current auth");
    let PrepareNext::Chunk(chunk) = harness
        .module
        .reads
        .next_snapshot(&harness.client, &read)
        .await
        .expect("prepared before real terminal")
    else {
        panic!("actual permit");
    };
    let mut gate = controller
        .arm_pause(
            ViewHookPoint::BeforeViewWorkerJoin,
            HookSelector::Source(harness.module.scope()),
        )
        .expect("actual cleanup boundary");
    harness.peer.close(deadline()).await;
    timeout_at(deadline(), gate.arrived())
        .await
        .expect("worker has not yet joined")
        .expect("actual point");
    assert!(
        harness.module.core.terminal.load(Ordering::Acquire),
        "the actual reader/IO terminal already ran"
    );
    assert!(
        matches!(
            writer
                .write_chunk(
                    chunk.into_local(),
                    authority.as_ref(),
                    &harness.client,
                    harness.target.key(),
                    deadline()
                )
                .await,
            Err(ViewError::StaleCursor)
        ),
        "source seal does not wait for display worker cleanup"
    );
    assert_eq!(
        stream
            .observation()
            .expect("actual send state")
            .highest_started,
        0
    );
    gate.release();
    drop(read);
    harness.close().await;
}
