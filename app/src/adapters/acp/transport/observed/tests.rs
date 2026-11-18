//! Developer IO tests. All observations originate in the actual reader.
use super::*;
use crate::adapters::acp::{
    Version,
    test_support::{Allow, parse_session, raw_value},
};
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(feature = "native-dispatch-dev-fixtures")]
include!("capacity_close_fault_tests.rs");

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
struct Attempt(std::sync::Arc<tokio::sync::Notify>);
impl WriteFence for Attempt {
    fn check_and_poll(
        &self,
        _: &WriteIdentity,
        _: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        self.0.notify_one();
        let _ = io.poll(cx);
        Ok(())
    }
}
type Peer = tokio::io::DuplexStream;
type InitializedPeer = (
    Connection,
    ObservationReceiver,
    ObservationAcknowledger,
    ObservedWriteGate,
    IoOwner,
    Peer,
);
async fn initialized(hooks: Arc<AcpObservationHooks>) -> InitializedPeer {
    initialized_with_writer(hooks, |writer| writer).await
}
async fn initialized_with_writer<W>(
    hooks: Arc<AcpObservationHooks>,
    decorate: impl FnOnce(tokio::io::WriteHalf<Peer>) -> W,
) -> InitializedPeer
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (local, mut peer) = tokio::io::duplex(16384);
    let (read, write) = tokio::io::split(local);
    let mut owner = None;
    let (start, mut receiver, ack, gate) =
        Connection::prepare_observed_with_hooks(read, decorate(write), hooks, |io| {
            owner = Some(io);
            Ok(())
        })
        .expect("prepare");
    let connection = start.start();
    let reservation = connection
        .reserve_initialize(Version::V1, deadline())
        .expect("reserve init");
    let id = reservation.id().expect("real rpc").clone();
    let frame = reservation.encode().expect("encode");
    let ticket = connection
        .enqueue(reservation, frame, Arc::new(Allow), deadline())
        .expect("enqueue");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    let mut line = String::new();
    BufReader::new(&mut peer)
        .read_line(&mut line)
        .await
        .expect("read init");
    let mut encoded = Vec::new();
    id.write_json(&mut encoded).expect("rpc encoding");
    let id = String::from_utf8(encoded).expect("utf8");
    peer.write_all(format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"protocolVersion\":1,\"agentCapabilities\":{{\"sessionCapabilities\":{{\"close\":{{}}}}}}}}}}\n").as_bytes()).await.expect("init response");
    let lease = receiver.next().await.expect("read").expect("init lease");
    ack.acknowledge(lease).expect("ack init");
    (
        connection,
        receiver,
        ack,
        gate,
        owner.expect("adopted owner"),
        peer,
    )
}
fn prompt(connection: &Connection) -> Reservation {
    let request = connection
        .strategy()
        .expect("strategy")
        .prompt(
            parse_session(&json!("synthetic-session")).expect("session"),
            raw_value(&json!([{"type":"text","text":"synthetic"}])),
            ExtensionFields::empty().expect("extra"),
        )
        .expect("prompt");
    connection
        .reserve_request(request, ResponseDeadline::Unbounded)
        .expect("reserve prompt")
}

fn close(connection: &Connection) -> Reservation {
    let request = connection
        .strategy()
        .expect("strategy")
        .close(
            parse_session(&json!("synthetic-session")).expect("session"),
            ExtensionFields::empty().expect("extra"),
        )
        .expect("close");
    connection
        .reserve_request(request, ResponseDeadline::Unbounded)
        .expect("reserve close")
}

#[tokio::test]
async fn automatic_close_waits_for_received_observations_before_first_byte() {
    let hooks = Arc::new(AcpObservationHooks::default());
    let (connection, mut receiver, ack, gate, owner, mut peer) = initialized(hooks.clone()).await;
    let held = hooks
        .pause_next(ObservationHookPoint::AfterReadCapturedBeforeRoute)
        .expect("hook");
    peer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"synthetic/unknown\"}\n")
        .await
        .expect("notification");
    held.wait_until_entered().await;
    let before = hooks.writer_polls();
    let notified = Arc::new(tokio::sync::Notify::new());
    let reservation = close(&connection);
    let frame = reservation.encode().expect("encode close");
    let ticket = connection
        .enqueue_automatic_close(
            reservation,
            frame,
            Arc::new(Attempt(notified.clone())),
            deadline(),
            &gate,
        )
        .expect("enqueue");
    notified.notified().await;
    assert_eq!(
        hooks.writer_polls(),
        before,
        "already received observations must precede automatic close IO"
    );
    held.release();
    let lease = receiver.next().await.expect("read").expect("lease");
    assert!(!matches!(ticket.progress(), WriteProgress::Finished(_)));
    ack.acknowledge(lease).expect("ack applied observation");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn explicit_close_still_writes_with_an_unapplied_observation() {
    let (connection, mut receiver, ack, _, owner, mut peer) = initialized(Arc::default()).await;
    peer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"synthetic/unknown\"}\n")
        .await
        .expect("notification");
    let lease = receiver.next().await.expect("read").expect("lease");
    let reservation = close(&connection);
    let frame = reservation.encode().expect("encode close");
    let ticket = connection
        .enqueue(reservation, frame, Arc::new(Allow), deadline())
        .expect("explicit close");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    ack.acknowledge(lease).expect("ack");
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn automatic_close_does_not_wait_for_later_observations_after_partial_write() {
    let (connection, mut receiver, ack, gate, owner, mut peer) = initialized(Arc::default()).await;
    let request = connection
        .strategy()
        .expect("strategy")
        .close(
            parse_session(&json!("synthetic-session")).expect("session"),
            ExtensionFields::from_raw(raw_value(&json!({"_meta":{"padding":"x".repeat(32768)}})))
                .expect("extra"),
        )
        .expect("close");
    let reservation = connection
        .reserve_request(request, ResponseDeadline::Unbounded)
        .expect("reserve");
    let frame = reservation.encode().expect("encode");
    let notified = Arc::new(tokio::sync::Notify::new());
    let ticket = connection
        .enqueue_automatic_close(
            reservation,
            frame,
            Arc::new(Attempt(notified.clone())),
            deadline(),
            &gate,
        )
        .expect("automatic close");
    notified.notified().await;
    assert!(matches!(ticket.progress(), WriteProgress::Writing { accepted, .. } if accepted > 0));
    peer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"synthetic/unknown\"}\n")
        .await
        .expect("later notification");
    let lease = receiver.next().await.expect("read").expect("lease");
    let mut line = String::new();
    tokio::time::timeout_at(deadline(), BufReader::new(&mut peer).read_line(&mut line))
        .await
        .expect("partial frame must finish without later ACK")
        .expect("read close");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    let value: serde_json::Value = serde_json::from_str(&line).expect("complete frame");
    assert_eq!(value["method"], "session/close");
    ack.acknowledge(lease).expect("ack later");
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn automatic_close_rejects_foreign_gate_and_non_close_preserving_inputs() {
    let (connection, _receiver, _ack, gate, owner, _peer) = initialized(Arc::default()).await;
    let (_foreign, _foreign_receiver, _foreign_ack, foreign_gate, foreign_owner, _foreign_peer) =
        initialized(Arc::default()).await;
    let reservation = close(&connection);
    let frame = reservation.encode().expect("frame");
    let (_, reservation, frame) = connection
        .enqueue_automatic_close(
            reservation,
            frame,
            Arc::new(Allow),
            deadline(),
            &foreign_gate,
        )
        .expect_err("foreign gate")
        .into_parts();
    let ticket = connection
        .enqueue_automatic_close(reservation, frame, Arc::new(Allow), deadline(), &gate)
        .expect("original close");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    let reservation = prompt(&connection);
    let frame = reservation.encode().expect("frame");
    let (_, reservation, frame) = connection
        .enqueue_automatic_close(reservation, frame, Arc::new(Allow), deadline(), &gate)
        .expect_err("wrong operation")
        .into_parts();
    let ticket = connection
        .enqueue_observed(reservation, frame, Arc::new(Allow), deadline(), &gate)
        .expect("original prompt");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    assert!(owner.shutdown(deadline()).await.joined);
    assert!(foreign_owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn actual_held_lf_and_unapplied_lease_keep_same_prompt_pending() {
    let hooks = Arc::new(AcpObservationHooks::default());
    let (connection, mut receiver, ack, gate, owner, mut peer) = initialized(hooks.clone()).await;
    let held = hooks
        .pause_next(ObservationHookPoint::AfterReadCapturedBeforeRoute)
        .expect("hook");
    peer.write_all(
        b"{\"jsonrpc\":\"2.0\",\"method\":\"synthetic/unknown\",\"params\":{\"future\":[1,2]}}\n",
    )
    .await
    .expect("peer write");
    held.wait_until_entered().await;
    let before = hooks.writer_polls();
    let notified = Arc::new(tokio::sync::Notify::new());
    let r = prompt(&connection);
    let probe = r.probe();
    let frame = r.encode().expect("frame");
    let ticket = connection
        .enqueue_observed(
            r,
            frame,
            Arc::new(Attempt(notified.clone())),
            deadline(),
            &gate,
        )
        .expect("observed");
    notified.notified().await;
    assert_eq!(hooks.writer_polls(), before);
    assert!(!matches!(probe.progress(), WriteProgress::Finished(_)));
    held.release();
    let lease = receiver.next().await.expect("read").expect("lease");
    assert!(
        lease
            .received()
            .raw()
            .field("params")
            .expect("unique decoded field")
            .is_some()
    );
    // Current safety control really writes while the original prompt is held by this lease.
    let cancel = connection
        .reserve_cancel(
            parse_session(&json!("synthetic-session")).expect("session"),
            ExtensionFields::empty().expect("extra"),
        )
        .expect("cancel");
    assert!(cancel.id().is_none());
    let f = cancel.encode().expect("encode cancel");
    let ct = connection
        .enqueue(cancel, f, Arc::new(Allow), deadline())
        .expect("control");
    assert_eq!(ct.wait().await, WriteReport::Written);
    assert!(!matches!(ticket.progress(), WriteProgress::Finished(_)));
    ack.acknowledge(lease).expect("apply ack");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn leases_ack_continuously_and_old_enqueue_cannot_bypass() {
    let (connection, mut receiver, ack, gate, owner, mut peer) = initialized(Arc::default()).await;
    peer.write_all(
        b"{\"jsonrpc\":\"2.0\",\"method\":\"one\"}\n{\"jsonrpc\":\"2.0\",\"method\":\"two\"}\n",
    )
    .await
    .expect("batch");
    let first = receiver.next().await.expect("read").expect("first");
    let second = receiver.next().await.expect("read").expect("second");
    let rejected = ack.acknowledge(second).expect_err("no skip").into_lease();
    assert_eq!(
        rejected.received().sequence(),
        first.received().sequence() + 1
    );
    let reservation = prompt(&connection);
    let frame = reservation.encode().expect("encode");
    let (_, reservation, frame) = connection
        .enqueue(reservation, frame, Arc::new(Allow), deadline())
        .expect_err("old bypass rejected")
        .into_parts();
    ack.acknowledge(first).expect("first ack");
    ack.acknowledge(rejected).expect("second ack");
    let ticket = connection
        .enqueue_observed(reservation, frame, Arc::new(Allow), deadline(), &gate)
        .expect("original inputs usable");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn dropping_unapplied_lease_stops_and_owner_really_joins() {
    let (connection, mut receiver, _, _, owner, mut peer) = initialized(Arc::default()).await;
    peer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"one\"}\n")
        .await
        .expect("write");
    drop(receiver.next().await.expect("read").expect("lease"));
    assert!(connection.is_closed());
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn prepared_owner_shutdown_before_start_waits_for_real_installation() {
    let (local, _peer) = tokio::io::duplex(1024);
    let (read, write) = tokio::io::split(local);
    let mut owner = None;
    let hooks = Arc::new(AcpObservationHooks::default());
    let (start, receiver, _ack, _gate) =
        Connection::prepare_observed_with_hooks(read, write, hooks.clone(), |io| {
            owner = Some(io);
            Ok(())
        })
        .expect("prepare");
    assert_eq!(hooks.writer_polls(), 0);
    let report = owner.expect("owned").shutdown(deadline()).await;
    assert!(report.joined);
    drop(start);
    drop(receiver);
    assert_eq!(hooks.writer_polls(), 0);
}

#[derive(Default)]
struct RequiredGap {
    required: bool,
    waker: Option<std::task::Waker>,
}
struct GapFence {
    state: Arc<std::sync::Mutex<RequiredGap>>,
    entered: Arc<tokio::sync::Notify>,
}
impl WriteFence for GapFence {
    fn check_and_poll(
        &self,
        _: &WriteIdentity,
        _: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let mut state = self.state.lock().expect("test gap mutex");
        if state.required {
            state.waker = Some(cx.waker().clone());
            io.park().expect("one proxy action");
        } else {
            let _ = io.poll(cx);
        }
        self.entered.notify_one();
        Ok(())
    }
}
fn clear_gap(state: &std::sync::Mutex<RequiredGap>) {
    let waker = {
        let mut state = state.lock().expect("test gap mutex");
        state.required = false;
        state.waker.take()
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}
#[tokio::test]
async fn application_gap_park_does_not_poll_provider_and_same_ticket_continues_after_control() {
    let hooks = Arc::new(AcpObservationHooks::default());
    let (connection, _receiver, _ack, gate, owner, mut peer) = initialized(hooks.clone()).await;
    let state = Arc::new(std::sync::Mutex::new(RequiredGap {
        required: true,
        waker: None,
    }));
    let entered = Arc::new(tokio::sync::Notify::new());
    let baseline = hooks.writer_polls();
    let reservation = prompt(&connection);
    let original = reservation.probe();
    let id = reservation.id().expect("original typed id").clone();
    let frame = reservation.encode().expect("original frame");
    let ticket = connection
        .enqueue_observed(
            reservation,
            frame,
            Arc::new(GapFence {
                state: state.clone(),
                entered: entered.clone(),
            }),
            deadline(),
            &gate,
        )
        .expect("same observed connection");
    entered.notified().await;
    assert_eq!(hooks.writer_polls(), baseline);
    assert!(matches!(
        ticket.progress(),
        WriteProgress::Writing { accepted: 0, .. }
    ));
    let control = connection
        .reserve_cancel(
            parse_session(&json!("synthetic-session")).expect("actual target"),
            ExtensionFields::empty().expect("extra"),
        )
        .expect("actual control");
    let frame = control.encode().expect("control frame");
    let control = connection
        .enqueue(control, frame, Arc::new(Allow), deadline())
        .expect("control enqueue");
    assert_eq!(control.wait().await, WriteReport::Written);
    let mut line = String::new();
    BufReader::new(&mut peer)
        .read_line(&mut line)
        .await
        .expect("control bytes");
    let value: serde_json::Value = serde_json::from_str(&line).expect("one complete control line");
    assert_eq!(value["method"], "session/cancel");
    assert!(value.get("id").is_none());
    assert!(!matches!(ticket.progress(), WriteProgress::Finished(_)));
    clear_gap(&state);
    assert_eq!(ticket.wait().await, WriteReport::Written);
    assert_eq!(
        original.progress(),
        WriteProgress::Finished(WriteReport::Written)
    );
    assert_eq!(original.id(), Some(&id));
    let mut line = String::new();
    BufReader::new(&mut peer)
        .read_line(&mut line)
        .await
        .expect("original prompt bytes");
    let value: serde_json::Value =
        serde_json::from_str(&line).expect("original complete prompt line");
    assert_eq!(value["method"], "session/prompt");
    assert!(owner.shutdown(deadline()).await.joined);
}
#[tokio::test]
async fn gap_clear_before_registration_does_not_lose_the_original_attempt() {
    let hooks = Arc::new(AcpObservationHooks::default());
    let (connection, _receiver, _ack, gate, owner, _peer) = initialized(hooks).await;
    let state = Arc::new(std::sync::Mutex::new(RequiredGap {
        required: true,
        waker: None,
    }));
    let reservation = prompt(&connection);
    let probe = reservation.probe();
    let frame = reservation.encode().expect("frame");
    // The writer cannot run on this current-thread runtime until after enqueue and clear.
    let ticket = connection
        .enqueue_observed(
            reservation,
            frame,
            Arc::new(GapFence {
                state: state.clone(),
                entered: Arc::new(tokio::sync::Notify::new()),
            }),
            deadline(),
            &gate,
        )
        .expect("enqueue");
    clear_gap(&state);
    assert_eq!(ticket.wait().await, WriteReport::Written);
    assert_eq!(
        probe.progress(),
        WriteProgress::Finished(WriteReport::Written)
    );
    assert!(owner.shutdown(deadline()).await.joined);
}
#[tokio::test]
async fn application_park_keeps_real_deadline_and_zero_classification() {
    let hooks = Arc::new(AcpObservationHooks::default());
    let (connection, _receiver, _ack, gate, owner, _peer) = initialized(hooks.clone()).await;
    let baseline = hooks.writer_polls();
    let state = Arc::new(std::sync::Mutex::new(RequiredGap {
        required: true,
        waker: None,
    }));
    let reservation = prompt(&connection);
    let frame = reservation.encode().expect("frame");
    let ticket = connection
        .enqueue_observed(
            reservation,
            frame,
            Arc::new(GapFence {
                state,
                entered: Arc::new(tokio::sync::Notify::new()),
            }),
            Instant::now() + Duration::from_millis(30),
            &gate,
        )
        .expect("enqueue");
    assert_eq!(
        ticket.wait().await,
        WriteReport::Zero {
            reason: ZeroReason::Deadline
        }
    );
    assert_eq!(hooks.writer_polls(), baseline);
    assert!(owner.shutdown(deadline()).await.joined);
}
