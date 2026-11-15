use super::*;
use crate::{
    adapters::acp::{
        ResponseDeadline, Version,
        raw_json::RawDocument,
        test_support::{Allow, setup},
    },
    application::shared_views::journal::MemoryJournal,
};
use futures_util::task::AtomicWaker;
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader, DuplexStream, WriteHalf},
    sync::Notify,
    time::{Instant, timeout_at},
};

struct FlushControl {
    phase: AtomicU8,
    waker: AtomicWaker,
    entered: Notify,
}
struct RealWriter {
    inner: WriteHalf<DuplexStream>,
    control: Arc<FlushControl>,
}
impl AsyncWrite for RealWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if self.control.phase.load(Ordering::Acquire) == 0 {
            return Pin::new(&mut self.inner).poll_flush(cx);
        }
        self.control.waker.register(cx.waker());
        self.control.entered.notify_one();
        if self.control.phase.load(Ordering::Acquire) == 2 {
            Poll::Ready(Err(std::io::Error::other(
                "self-owned fixture flush failure",
            )))
        } else {
            Poll::Pending
        }
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
fn response(id: &RpcId, result: &[u8]) -> Vec<u8> {
    let mut bytes = br#"{"jsonrpc":"2.0","id":"#.to_vec();
    id.write_json(&mut bytes).expect("typed original RPC");
    bytes.extend_from_slice(br#","result":"#);
    bytes.extend_from_slice(result);
    bytes.extend_from_slice(b"}\n");
    bytes
}
#[allow(
    clippy::too_many_lines,
    reason = "Keep the same real reservation, byte peer, pending flush, Received and later error in one visible causal test"
)]
async fn replay_before_flush_error(version: Version) {
    let deadline = || Instant::now() + Duration::from_secs(5);
    let (node, peer) = tokio::io::duplex(4096);
    let (nr, nw) = tokio::io::split(node);
    let (pr, mut pw) = tokio::io::split(peer);
    let mut pr = BufReader::new(pr);
    let control = Arc::new(FlushControl {
        phase: AtomicU8::new(0),
        waker: AtomicWaker::new(),
        entered: Notify::new(),
    });
    let (connection, mut rx, owner) = Connection::spawn(
        nr,
        RealWriter {
            inner: nw,
            control: Arc::clone(&control),
        },
    )
    .expect("actual original IO");
    let (mut capture, mut consumer, probe) =
        super::super::shared_view_capture::AcpViewCapture::for_connection(&connection);
    let initialize = connection
        .reserve_initialize(version, deadline())
        .expect("real init reserve");
    let id = initialize.id().expect("init rpc").clone();
    let encoded = initialize.encode().expect("encode");
    assert!(matches!(
        capture.track_replay(&initialize, ReadEpoch::fresh().expect("epoch")),
        Err(ViewError::InvalidTarget)
    ));
    let init = connection
        .enqueue(initialize, encoded, Arc::new(Allow), deadline())
        .expect("init IO");
    let mut line = Vec::new();
    pr.read_until(b'\n', &mut line)
        .await
        .expect("actual peer sees init");
    let negotiated:&[u8]=match version {
        Version::V1=>br#"{"protocolVersion":1,"agentInfo":{"name":"fixture","version":"1"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}}}"#,
        Version::V2=>br#"{"protocolVersion":2,"info":{"name":"fixture","version":"2"},"capabilities":{"session":{}}}"#,
    };
    pw.write_all(&response(&id, negotiated))
        .await
        .expect("real negotiated capability response");
    let event = timeout_at(deadline(), rx.next())
        .await
        .expect("wait")
        .expect("read")
        .expect("actual init");
    capture.capture_received(&event).expect("capture");
    assert!(matches!(init.wait().await, WriteReport::Written));
    let strategy = connection.strategy().expect("actual negotiated strategy");
    let session = SessionId::parse(
        RawDocument::parse(br#""native-replay""#.to_vec())
            .expect("session value")
            .root(),
    )
    .expect("session");
    let resume = connection
        .reserve_request(
            strategy
                .resume(session.clone(), &setup(version, "C:\\fixture"))
                .expect("real distinct Resume request"),
            ResponseDeadline::Unbounded,
        )
        .expect("actual Resume reservation");
    assert!(matches!(
        capture.track_replay(&resume, ReadEpoch::fresh().expect("epoch")),
        Err(ViewError::InvalidTarget)
    ));
    assert!(
        matches!(
            resume.cancel().progress(),
            WriteProgress::Finished(WriteReport::Zero { .. })
        ),
        "same never-enqueued Resume reservation cancelled without IO"
    );
    let replay = connection
        .reserve_request(
            strategy
                .replay(session, &setup(version, "C:\\fixture"))
                .expect("standard replay request"),
            ResponseDeadline::Unbounded,
        )
        .expect("same source reserve");
    let rpc = replay.id().expect("actual replay RPC").clone();
    let epoch = ReadEpoch::fresh().expect("distinct read epoch");
    let mut facts = capture
        .track_replay(&replay, epoch)
        .expect("real scope+RequestKind::Replay");
    assert_eq!(
        facts.observation().transport(),
        ReplayTransportState::Reserved
    );
    let frame = replay.encode().expect("actual frame");
    control.phase.store(1, Ordering::Release);
    let ticket = connection
        .enqueue(replay, frame, Arc::new(Allow), deadline())
        .expect("replay enqueue");
    line.clear();
    pr.read_until(b'\n', &mut line)
        .await
        .expect("real replay reaches peer");
    match version {
        Version::V1 => assert!(line.windows(12).any(|v| v == b"session/load")),
        Version::V2 => {
            assert!(line.windows(14).any(|v| v == b"session/resume"));
            assert!(line.windows(12).any(|v| v == b"\"replayFrom\""));
        }
    }
    timeout_at(deadline(), control.entered.notified())
        .await
        .expect("actual replay flush is Pending");
    pw.write_all(br#"{"jsonrpc":"2.0","method":"future","params":{"unknown":123456789012345678901234567890}}"#).await.expect("actual raw before completion");
    pw.write_all(b"\n").await.expect("LF");
    let update = timeout_at(deadline(), rx.next())
        .await
        .expect("wait")
        .expect("read")
        .expect("actual unknown");
    capture.capture_received(&update).expect("retain original");
    assert!(
        !facts
            .observe_received(&update)
            .expect("non-response cannot complete")
    );
    if version == Version::V2 {
        pw.write_all(br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"native-replay","update":{"sessionUpdate":"state_update","state":"idle"}}}"#).await.expect("actual mandatory V2 state snapshot");
        pw.write_all(b"\n").await.expect("LF");
        let snapshot = timeout_at(deadline(), rx.next())
            .await
            .expect("state wait")
            .expect("state read")
            .expect("actual state");
        assert!(matches!(
            snapshot.incoming(),
            Incoming::Update {
                origin: super::super::UpdateOrigin::DuringReplay,
                ..
            }
        ));
        capture
            .capture_received(&snapshot)
            .expect("same source retains state raw");
        assert!(
            !facts
                .observe_received(&snapshot)
                .expect("state snapshot is not Replay completion")
        );
        assert_eq!(
            facts.observation().completion(),
            ReplayCompletionState::AwaitingResponse
        );
    }
    pw.write_all(&response(&rpc, b"{}"))
        .await
        .expect("real Replay completion before flush");
    let complete = timeout_at(deadline(), rx.next())
        .await
        .expect("wait")
        .expect("read")
        .expect("actual Replayed");
    assert!(matches!(
        complete.incoming(),
        Incoming::Response {
            response: Response::Replayed { .. },
            ..
        }
    ));
    assert_eq!(
        facts.observe_received(&complete),
        Err(ViewError::CaptureGap),
        "never accept a response before same source captured it"
    );
    capture.capture_received(&complete).expect("capture once");
    assert!(
        facts
            .observe_received(&complete)
            .expect("real matched receipt")
    );
    assert_eq!(
        facts.observation().completion(),
        ReplayCompletionState::AwaitingCapture
    );
    let mut journal = MemoryJournal::new().expect("no-budget journal");
    consumer
        .drain_into(&mut journal)
        .expect("all actual raw before response processed");
    probe
        .wait_processed(complete.sequence())
        .await
        .expect("actual prefix");
    let before = facts.observation();
    assert_eq!(before.completion(), ReplayCompletionState::Complete);
    assert_eq!(before.transport(), ReplayTransportState::Writing);
    assert_eq!(before.freshness(), ReplayFreshness::CurrentObservation);
    assert_eq!(before.read_epoch(), epoch);
    control.phase.store(2, Ordering::Release);
    control.waker.wake();
    assert!(
        matches!(ticket.wait().await, WriteReport::Unknown { .. }),
        "later true writer flush failure"
    );
    assert!(
        owner.shutdown(deadline()).await.joined,
        "original IO is actually joined"
    );
    let after = facts.observation();
    assert_eq!(after.transport(), ReplayTransportState::Unknown);
    assert_eq!(after.freshness(), ReplayFreshness::Stale);
    assert_eq!(after.completion(), ReplayCompletionState::Complete);
    assert_eq!(after.failure(), None);
}

#[tokio::test]
async fn shared_view_replay_v1_real_response_before_flush_err_keeps_completion_and_marks_stale() {
    replay_before_flush_error(Version::V1).await;
}
#[tokio::test]
async fn shared_view_replay_v2_real_response_before_flush_err_keeps_completion_and_marks_stale() {
    replay_before_flush_error(Version::V2).await;
}
