use crate::adapters::acp::WriteFailure;
use crate::adapters::acp::capacity_close_write_fault::CloseWriteFaultMode;
use std::task::Poll;

async fn capacity_close_fault_report(mode: CloseWriteFaultMode, drop_claimed_guard: bool) {
    use tokio::io::AsyncReadExt as _;
    let hooks = Arc::new(AcpObservationHooks::default());
    let (connection, _receiver, _ack, gate, owner, mut peer) = initialized(hooks.clone()).await;
    let baseline = hooks.writer_polls();
    let guard = connection
        .capacity_install_close_write_fault("synthetic-session", mode)
        .expect("original connection fault");
    let reservation = close(&connection);
    let probe = reservation.probe();
    let frame = reservation.encode().expect("real close frame");
    let expected_bytes = frame.bytes.clone();
    let ticket = connection
        .enqueue_automatic_close(reservation, frame, Arc::new(Allow), deadline(), &gate)
        .expect("original automatic close ticket");
    assert!(guard.observation().claimed);
    let guard = if drop_claimed_guard {
        drop(guard);
        None
    } else {
        Some(guard)
    };
    let report = tokio::time::timeout_at(deadline(), ticket.wait())
        .await
        .expect("original ticket bounded");
    match mode {
        CloseWriteFaultMode::BeforeWrite => {
            assert_eq!(
                report,
                WriteReport::Zero {
                    reason: ZeroReason::FenceRejected
                }
            );
            assert_eq!(hooks.writer_polls(), baseline);
        }
        CloseWriteFaultMode::AfterOneByte => {
            assert_eq!(
                report,
                WriteReport::Unknown {
                    accepted: 1,
                    cause: WriteFailure::Io
                }
            );
        }
        CloseWriteFaultMode::BeforeFlush => {
            assert_eq!(
                report,
                WriteReport::Unknown {
                    accepted: expected_bytes.len(),
                    cause: WriteFailure::Io
                }
            );
        }
    }
    assert_eq!(probe.progress(), WriteProgress::Finished(report));
    if let Some(guard) = guard {
        let observed = guard.observation();
        assert!(observed.claimed && observed.fired);
        assert_eq!(observed.total, expected_bytes.len());
        assert_eq!(
            observed.accepted,
            match mode {
                CloseWriteFaultMode::BeforeWrite => 0,
                CloseWriteFaultMode::AfterOneByte => 1,
                CloseWriteFaultMode::BeforeFlush => expected_bytes.len(),
            }
        );
    }
    assert!(owner.shutdown(deadline()).await.joined);
    let mut actual = Vec::new();
    tokio::time::timeout_at(deadline(), peer.read_to_end(&mut actual))
        .await
        .expect("owned pipe drained")
        .expect("peer read");
    match mode {
        CloseWriteFaultMode::BeforeWrite => assert!(actual.is_empty()),
        CloseWriteFaultMode::AfterOneByte => assert_eq!(actual, expected_bytes[..1]),
        CloseWriteFaultMode::BeforeFlush => assert_eq!(actual, expected_bytes),
    }
}

#[tokio::test]
async fn capacity_close_fault_zero_never_polls_actual_provider() {
    capacity_close_fault_report(CloseWriteFaultMode::BeforeWrite, false).await;
}

#[tokio::test]
async fn capacity_close_fault_partial_retains_actual_one_byte_and_unknown() {
    capacity_close_fault_report(CloseWriteFaultMode::AfterOneByte, false).await;
}

#[tokio::test]
async fn capacity_close_fault_full_frame_is_unknown_until_flush() {
    capacity_close_fault_report(CloseWriteFaultMode::BeforeFlush, false).await;
}

#[tokio::test]
async fn capacity_close_fault_claim_survives_installing_guard_drop() {
    capacity_close_fault_report(CloseWriteFaultMode::AfterOneByte, true).await;
}

#[tokio::test]
async fn capacity_close_fault_unclaimed_guard_drop_preserves_normal_close() {
    let (connection, _receiver, _ack, gate, owner, mut peer) = initialized(Arc::default()).await;
    let guard = connection
        .capacity_install_close_write_fault("synthetic-session", CloseWriteFaultMode::BeforeWrite)
        .expect("pending fault");
    assert!(!guard.observation().claimed);
    drop(guard);
    let reservation = close(&connection);
    let frame = reservation.encode().expect("frame");
    let ticket = connection
        .enqueue_automatic_close(reservation, frame, Arc::new(Allow), deadline(), &gate)
        .expect("normal close");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    let mut line = String::new();
    tokio::time::timeout_at(deadline(), BufReader::new(&mut peer).read_line(&mut line))
        .await
        .expect("actual full frame")
        .expect("peer read");
    assert!(line.contains("session/close"));
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn capacity_close_fault_does_not_claim_explicit_or_foreign_close() {
    let (connection, _receiver, _ack, gate, owner, _peer) = initialized(Arc::default()).await;
    let guard = connection
        .capacity_install_close_write_fault("different-native", CloseWriteFaultMode::BeforeWrite)
        .expect("other target");
    let reservation = close(&connection);
    let frame = reservation.encode().expect("frame");
    let ticket = connection
        .enqueue_automatic_close(reservation, frame, Arc::new(Allow), deadline(), &gate)
        .expect("unrelated automatic close");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    assert!(!guard.observation().claimed);
    drop(guard);
    let guard = connection
        .capacity_install_close_write_fault("synthetic-session", CloseWriteFaultMode::BeforeWrite)
        .expect("matching target");
    let reservation = close(&connection);
    let frame = reservation.encode().expect("frame");
    let ticket = connection
        .enqueue(reservation, frame, Arc::new(Allow), deadline())
        .expect("explicit close remains ordinary");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    assert!(!guard.observation().claimed);
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn capacity_close_fault_does_not_follow_same_native_to_another_connection() {
    let (original, _rx, _ack, _gate, owner, _peer) = initialized(Arc::default()).await;
    let (other, _other_rx, _other_ack, other_gate, other_owner, _other_peer) =
        initialized(Arc::default()).await;
    let guard = original
        .capacity_install_close_write_fault("synthetic-session", CloseWriteFaultMode::BeforeWrite)
        .expect("original connection only");
    let reservation = close(&other);
    let frame = reservation
        .encode()
        .expect("same native on foreign connection");
    let ticket = other
        .enqueue_automatic_close(reservation, frame, Arc::new(Allow), deadline(), &other_gate)
        .expect("unrelated connection close");
    assert_eq!(ticket.wait().await, WriteReport::Written);
    assert!(!guard.observation().claimed);
    assert!(other_owner.shutdown(deadline()).await.joined);
    assert!(owner.shutdown(deadline()).await.joined);
}

#[tokio::test]
async fn capacity_close_fault_claimed_command_cancel_uses_original_zero() {
    let hooks = Arc::new(AcpObservationHooks::default());
    let (connection, _rx, _ack, gate, owner, _peer) = initialized(hooks.clone()).await;
    let baseline = hooks.writer_polls();
    let guard = connection
        .capacity_install_close_write_fault("synthetic-session", CloseWriteFaultMode::AfterOneByte)
        .expect("fault");
    let reservation = close(&connection);
    let frame = reservation.encode().expect("frame");
    let ticket = connection
        .enqueue_automatic_close(reservation, frame, Arc::new(Allow), deadline(), &gate)
        .expect("original queued command");
    assert!(guard.observation().claimed);
    ticket.request_cancel();
    assert_eq!(
        ticket.wait().await,
        WriteReport::Zero {
            reason: ZeroReason::Cancelled
        }
    );
    assert!(!guard.observation().fired);
    assert_eq!(hooks.writer_polls(), baseline);
    assert!(owner.shutdown(deadline()).await.joined);
}

struct PendingCloseWriter<W> {
    actual: W,
    armed: Arc<std::sync::atomic::AtomicBool>,
    guard: Arc<
        std::sync::Mutex<
            Option<crate::adapters::acp::capacity_close_write_fault::CloseWriteFaultGuard>,
        >,
    >,
}

impl<W: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for PendingCloseWriter<W> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        let result = std::pin::Pin::new(&mut self.actual).poll_write(cx, bytes);
        if matches!(result, Poll::Ready(Ok(1))) {
            // Drop the installer after the real provider accepted the prefix.
            // The original command must still retain its next-poll fault.
            drop(self.guard.lock().expect("test guard lock").take());
        }
        result
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.actual).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.actual).poll_shutdown(cx)
    }
}

#[tokio::test]
async fn capacity_close_fault_pending_then_prefix_keeps_fault_after_guard_drop() {
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use tokio::io::AsyncReadExt as _;
    let armed = Arc::new(AtomicBool::new(false));
    let held = Arc::new(Mutex::new(None));
    let (connection, _rx, _ack, gate, owner, mut peer) =
        initialized_with_writer(Arc::default(), |actual| PendingCloseWriter {
            actual,
            armed: armed.clone(),
            guard: held.clone(),
        })
        .await;
    let guard = connection
        .capacity_install_close_write_fault("synthetic-session", CloseWriteFaultMode::AfterOneByte)
        .expect("fault");
    *held.lock().expect("hold guard") = Some(guard);
    armed.store(true, Ordering::SeqCst);
    let reservation = close(&connection);
    let frame = reservation.encode().expect("frame");
    let first = frame.bytes()[0];
    let ticket = connection
        .enqueue_automatic_close(reservation, frame, Arc::new(Allow), deadline(), &gate)
        .expect("original ticket");
    assert_eq!(
        tokio::time::timeout_at(deadline(), ticket.wait())
            .await
            .expect("ticket"),
        WriteReport::Unknown {
            accepted: 1,
            cause: WriteFailure::Io
        }
    );
    assert!(!armed.load(Ordering::SeqCst));
    assert!(
        held.lock()
            .expect("installer dropped by actual provider")
            .is_none()
    );
    assert!(owner.shutdown(deadline()).await.joined);
    let mut actual = Vec::new();
    tokio::time::timeout_at(deadline(), peer.read_to_end(&mut actual))
        .await
        .expect("pipe ended")
        .expect("read");
    assert_eq!(actual, vec![first]);
}

#[tokio::test]
async fn resume_enqueue_fault_is_scoped_once_and_clears_on_drop() {
    use crate::adapters::acp::test_support::setup;
    use crate::adapters::acp::transport::capacity_close_write_fault::claim_resume_enqueue_fault;
    // 构造支持 session/resume 能力的 V2 已握手连接，专门验证单次 Resume 故障守卫。
    let (local, mut peer) = tokio::io::duplex(16384);
    let (read, write) = tokio::io::split(local);
    let mut owner = None;
    let (start, mut receiver, ack, _gate) = Connection::prepare_observed_with_hooks(
        read,
        write,
        Arc::default(),
        |io| {
            owner = Some(io);
            Ok(())
        },
    )
    .expect("prepare");
    let connection = start.start();
    let owner = owner.expect("adopted owner");
    let init = connection
        .reserve_initialize(Version::V2, deadline())
        .expect("reserve init");
    let id = init.id().expect("real rpc").clone();
    let frame = init.encode().expect("encode");
    let ticket = connection
        .enqueue(init, frame, Arc::new(Allow), deadline())
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
    peer.write_all(
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"protocolVersion\":2,\"info\":{{\"name\":\"fixture\",\"version\":\"fixed\"}},\"capabilities\":{{\"session\":{{}}}}}}}}\n"
        )
        .as_bytes(),
    )
    .await
    .expect("init response");
    let lease = receiver.next().await.expect("read").expect("init lease");
    ack.acknowledge(lease).expect("ack init");

    let strategy = connection.strategy().expect("strategy");
    let session_setup = setup(strategy.version(), "C:\\unused");
    let make_resume = |name: &str| {
        let req = strategy
            .resume(
                parse_session(&json!(name)).expect("session"),
                &session_setup,
            )
            .expect("resume request");
        connection
            .reserve_request(req, ResponseDeadline::Unbounded)
            .expect("reservation")
    };

    // 1. 未消费时 Drop 应自动清除，不能影响后续同一会话
    let guard = connection
        .capacity_install_resume_enqueue_fault("target-root")
        .expect("install guard");
    assert!(!guard.observation().claimed);
    drop(guard);
    assert!(!claim_resume_enqueue_fault(&connection, &make_resume("target-root")));

    // 2. 安装后不能误伤其他会话，仅匹配目标触发一次，第二次不再拦截
    let guard = connection
        .capacity_install_resume_enqueue_fault("target-root")
        .expect("reinstall guard");
    assert!(!claim_resume_enqueue_fault(&connection, &make_resume("other-root")));
    assert!(!guard.observation().claimed);
    assert!(claim_resume_enqueue_fault(&connection, &make_resume("target-root")));
    assert!(guard.observation().claimed && guard.observation().fired);
    assert!(!claim_resume_enqueue_fault(&connection, &make_resume("target-root")));
    drop(guard);
    assert!(owner.shutdown(deadline()).await.joined);
}
