#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::Context,
    time::Duration,
};
use support::*;
use tokio::time::{Instant, timeout};

struct DropReceiverFence {
    receiver: Mutex<Option<EventReceiver>>,
    at_flush: bool,
}
impl WriteFence for DropReceiverFence {
    fn check_and_poll(
        &self,
        _: &WriteIdentity,
        step: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let actual = io.poll(cx);
        let cut = if self.at_flush {
            matches!(step, WriteStep::Flush { .. })
                && matches!(actual, std::task::Poll::Ready(Ok(IoProgress::Flushed)))
        } else {
            matches!(actual, std::task::Poll::Ready(Err(_)))
        };
        if cut {
            drop(self.receiver.lock().unwrap().take());
        } // Real public owner Stop, after actual poll but before fence returns.
        Ok(())
    }
}

#[tokio::test]
async fn writer001_actual_error_or_flush_then_receiver_drop_inside_legal_fence() {
    for at_flush in [false, true] {
        let r = Rig::ready(Version::V2).await;
        let request = r.prompt();
        let Rig {
            client,
            receiver,
            owner,
            read: _,
            write,
            read_weak,
            write_weak,
        } = r;
        if !at_flush {
            write.mode(WriteMode::Fail);
        }
        let fence = Arc::new(DropReceiverFence {
            receiver: Mutex::new(Some(receiver)),
            at_flush,
        });
        let res = client
            .reserve_request(request, ResponseDeadline::Unbounded)
            .unwrap();
        let frame = res.encode().unwrap();
        let t = client.enqueue(res, frame, fence, deadline()).unwrap();
        let result = t.wait().await;
        if at_flush {
            assert!(matches!(result, WriteReport::Written));
            assert!(write.bytes().ends_with(b"\n"));
        } else {
            assert!(matches!(
                result,
                WriteReport::Unknown {
                    accepted: 0,
                    cause: WriteFailure::Io
                }
            ));
            assert!(write.bytes().is_empty());
        }
        assert!(client.is_closed());
        assert!(owner.shutdown(deadline()).await.joined);
        assert!(read_weak.upgrade().is_none());
        assert!(write_weak.upgrade().is_none());
    }
}

struct InvalidAfterFlush;
impl WriteFence for InvalidAfterFlush {
    fn check_and_poll(
        &self,
        _: &WriteIdentity,
        step: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let _ = io.poll(cx);
        if matches!(step, WriteStep::Flush { .. }) {
            let _ = io.poll(cx);
            Err(FenceRejection::Stopped)
        } else {
            Ok(())
        }
    }
}
#[tokio::test]
async fn io14_invalid_after_real_flush_preserves_transport_written_but_closes_with_error() {
    let mut r = Rig::ready(Version::V2).await;
    let flushes = r.write.flushes();
    let res = r.reserve_prompt();
    let frame = res.encode().unwrap();
    let t = r
        .client
        .enqueue(res, frame, Arc::new(InvalidAfterFlush), deadline())
        .unwrap();
    assert!(matches!(t.wait().await, WriteReport::Written));
    assert!(r.client.is_closed());
    assert_eq!(r.write.flushes() - flushes, 1);
    assert!(matches!(r.next().await, Err(AcpError::InvalidFence)));
    assert!(r.next().await.unwrap().is_none());
    r.close().await;
}

struct RecordFence {
    reject: AtomicBool,
    steps: Mutex<Vec<WriteStep>>,
}
impl WriteFence for RecordFence {
    fn check_and_poll(
        &self,
        id: &WriteIdentity,
        step: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let _ = id.scope();
        let _ = id.id();
        let _ = id.kind();
        let _ = id.setup_stamp();
        self.steps.lock().unwrap().push(step);
        if self.reject.load(Ordering::SeqCst) {
            return Err(FenceRejection::RevisionChanged);
        }
        let _ = io.poll(cx);
        Ok(())
    }
}
#[tokio::test]
async fn io13_every_pending_poll_rechecks_fence_and_flush_is_fenced() {
    let r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
    let baseline = r.write.polls();
    let f = Arc::new(RecordFence {
        reject: AtomicBool::new(false),
        steps: Mutex::new(vec![]),
    });
    let res = r.reserve_prompt();
    let frame = res.encode().unwrap();
    let t = r.client.enqueue(res, frame, f.clone(), deadline()).unwrap();
    r.write.wait_for(|w| w.polls() > baseline).await;
    f.reject.store(true, Ordering::SeqCst);
    r.write.mode(WriteMode::Open);
    assert!(matches!(
        t.wait().await,
        WriteReport::Zero {
            reason: ZeroReason::FenceRejected
        }
    ));
    assert!(r.write.bytes().is_empty());
    assert!(f.steps.lock().unwrap().len() >= 2);
    let f = Arc::new(RecordFence {
        reject: AtomicBool::new(false),
        steps: Mutex::new(vec![]),
    });
    let res = r.reserve_prompt();
    let frame = res.encode().unwrap();
    let t = r.client.enqueue(res, frame, f.clone(), deadline()).unwrap();
    finish(&t).await;
    {
        let steps = f.steps.lock().unwrap();
        assert!(matches!(
            steps.first(),
            Some(WriteStep::Chunk { offset: 0, .. })
        ));
        assert!(matches!(steps.last(), Some(WriteStep::Flush { .. })));
        drop(steps);
    }
    r.close().await;
}
struct Invalid(u8);
impl WriteFence for Invalid {
    fn check_and_poll(
        &self,
        _: &WriteIdentity,
        _: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        match self.0 {
            0 => Ok(()),
            1 => {
                let _ = io.poll(cx);
                let _ = io.poll(cx);
                Ok(())
            }
            _ => {
                let _ = io.poll(cx);
                Err(FenceRejection::Stopped)
            }
        }
    }
}
#[tokio::test]
async fn io14_invalid_fence_does_not_fabricate_poll_or_written() {
    for mode in 0..3 {
        let r = Rig::ready(Version::V2).await;
        let before = r.write.polls();
        let res = r.reserve_prompt();
        let frame = res.encode().unwrap();
        let total = frame.bytes().len();
        let t = r
            .client
            .enqueue(res, frame, Arc::new(Invalid(mode)), deadline())
            .unwrap();
        let result = t.wait().await;
        assert!(!matches!(result, WriteReport::Written));
        assert!(r.client.is_closed());
        assert_eq!(r.write.polls() - before, usize::from(mode != 0));
        assert_eq!(r.write.bytes().len(), if mode == 0 { 0 } else { total });
        r.close().await;
    }
}
#[tokio::test]
async fn io16_pending_zero_control_precedes_same_ordinary_cell() {
    let mut r = Rig::ready(Version::V1).await;
    r.write.mode(WriteMode::Pending);
    let count = r.write.polls();
    let regular = send(&r.client, r.reserve_prompt());
    r.write.wait_for(|w| w.polls() > count).await;
    let recv = r.feed(permission(Version::V1, &RpcId::Integer(600))).await;
    let Incoming::Permission(p) = recv.incoming() else {
        panic!()
    };
    let control = r
        .client
        .reserve_permission_response(p, PermissionSelection::Cancelled)
        .unwrap();
    let control = send(&r.client, control);
    r.write.mode(WriteMode::Open);
    finish(&control).await;
    finish(&regular).await;
    let bytes = r.write.bytes();
    let lines = bytes
        .split(|&b| b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice::<serde_json::Value>(b).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["id"], 600);
    assert!(lines[0].get("result").is_some());
    assert_eq!(lines[1]["method"], "session/prompt");
    r.close().await;
}
#[tokio::test]
async fn io16_partial_frame_never_interleaves_control_bytes() {
    let mut r = Rig::ready(Version::V1).await;
    r.write.mode(WriteMode::Limit(5));
    let regular = send(&r.client, r.reserve_prompt());
    r.write.wait_for(|w| w.bytes().len() == 5).await;
    let recv = r.feed(permission(Version::V1, &RpcId::Integer(601))).await;
    let Incoming::Permission(p) = recv.incoming() else {
        panic!()
    };
    let control = send(
        &r.client,
        r.client
            .reserve_permission_response(p, PermissionSelection::Cancelled)
            .unwrap(),
    );
    assert!(matches!(
        regular.cancel_and_wait().await,
        WriteReport::Unknown { accepted: 5, .. }
    ));
    assert!(matches!(control.wait().await, WriteReport::Zero { .. }));
    r.close().await;
    assert_eq!(r.write.bytes().len(), 5);
}
#[tokio::test]
async fn rc03_same_read_response_then_update_closes_window_and_shares_cut() {
    let mut r = Rig::ready(Version::V2).await;
    let req = r
        .client
        .strategy()
        .unwrap()
        .resume(sid("A"), &setup(Version::V2, "C:/synthetic"))
        .unwrap();
    let res = r
        .client
        .reserve_request(req, ResponseDeadline::Unbounded)
        .unwrap();
    let id = res.id().unwrap().clone();
    let t = send(&r.client, res);
    finish(&t).await;
    let state = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"A","update":{"sessionUpdate":"state_update","state":"idle"}}});
    let response = json!({"jsonrpc":"2.0","id":rpc_value(&id),"result":{}});
    let update = notification("A");
    let batch = format!(
        "{}\n{}\n{}\n",
        state,
        response,
        std::str::from_utf8(&update).unwrap()
    );
    r.read.push(batch.into_bytes());
    let first = r.next().await.unwrap().unwrap();
    let second = r.next().await.unwrap().unwrap();
    let third = r.next().await.unwrap().unwrap();
    assert!(matches!(
        first.incoming(),
        Incoming::Update {
            origin: UpdateOrigin::DuringResume,
            ..
        }
    ));
    assert!(matches!(second.incoming(), Incoming::Response { .. }));
    assert!(matches!(
        third.incoming(),
        Incoming::Update {
            origin: UpdateOrigin::Live,
            ..
        }
    ));
    assert_eq!(first.receipt_order(), second.receipt_order());
    assert_eq!(second.receipt_order(), third.receipt_order());
    assert!(first.sequence() < second.sequence() && second.sequence() < third.sequence());
    r.close().await;
}
#[tokio::test]
async fn rc04_receipt_uses_final_lf_batch_not_prefix_batch() {
    let mut r = Rig::ready(Version::V2).await;
    let wire = notification("A");
    let baseline = r.read.batches();
    r.read.push(wire);
    r.read.wait_batches(baseline + 1).await;
    let req = r
        .client
        .strategy()
        .unwrap()
        .resume(sid("A"), &setup(Version::V2, "C:/synthetic"))
        .unwrap();
    let t = send(
        &r.client,
        r.client
            .reserve_request(req, ResponseDeadline::Unbounded)
            .unwrap(),
    );
    finish(&t).await;
    r.read.push(vec![b'\n']);
    let received = r.next().await.unwrap().unwrap();
    assert!(matches!(
        received.incoming(),
        Incoming::Update {
            origin: UpdateOrigin::DuringResume,
            ..
        }
    ));
    r.close().await;
}
#[tokio::test]
async fn dl02_queued_pending_zero_response_expiry_without_consumer() {
    let r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
    let res = r
        .client
        .reserve_request(
            r.prompt(),
            ResponseDeadline::At(Instant::now() + Duration::from_millis(150)),
        )
        .unwrap();
    let t = send(&r.client, res);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(matches!(
        t.progress(),
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Deadline
        })
    ));
    assert!(r.write.bytes().is_empty());
    assert_eq!(r.client.pending_count(), 0);
    r.close().await;
}
#[tokio::test]
async fn dl03_partial_expiry_dl04_full_unflushed_and_dl05_written() {
    for state in 0..3 {
        let r = Rig::ready(Version::V2).await;
        if state == 0 {
            r.write.mode(WriteMode::Limit(3));
        } else if state == 1 {
            r.write.flush(WriteMode::Pending);
        }
        let res = r
            .client
            .reserve_request(
                r.prompt(),
                ResponseDeadline::At(Instant::now() + Duration::from_millis(150)),
            )
            .unwrap();
        let id = res.id().unwrap().clone();
        let t = send(&r.client, res);
        if state == 0 {
            r.write.wait_for(|w| w.bytes().len() == 3).await;
        } else {
            r.write.wait_for(|w| w.bytes().ends_with(b"\n")).await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        let result = t.wait().await;
        if state == 2 {
            assert!(matches!(result, WriteReport::Written));
        } else {
            assert!(matches!(
                result,
                WriteReport::Unknown {
                    cause: WriteFailure::Deadline,
                    ..
                }
            ));
        }
        assert!(r.client.is_closed());
        let report = r.owner.shutdown(deadline()).await;
        assert!(report.joined);
        assert!(report.unresolved_requests.contains(&id));
        r.close().await;
    }
}
#[tokio::test]
async fn dl06_response_removes_response_deadline_but_flush_still_has_write_deadline() {
    let mut r = Rig::ready(Version::V2).await;
    r.write.flush(WriteMode::Pending);
    let res = r
        .client
        .reserve_request(
            r.prompt(),
            ResponseDeadline::At(Instant::now() + Duration::from_millis(500)),
        )
        .unwrap();
    let id = res.id().unwrap().clone();
    let frame = res.encode().unwrap();
    let t = r
        .client
        .enqueue(
            res,
            frame,
            Arc::new(Fence),
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
    r.write.wait_for(|w| w.bytes().ends_with(b"\n")).await;
    assert!(matches!(
        r.respond(&id, json!({"messageId":"inserted"}))
            .await
            .incoming(),
        Incoming::Response { .. }
    ));
    tokio::time::sleep(Duration::from_millis(650)).await;
    assert!(!r.client.is_closed());
    assert!(matches!(t.progress(), WriteProgress::Flushing { .. }));
    r.write.flush(WriteMode::Open);
    finish(&t).await;
    r.close().await;
}
#[tokio::test]
async fn io07_write_io_error_even_at_zero_is_not_zero_success() {
    let r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Fail);
    let t = send(&r.client, r.reserve_prompt());
    assert!(matches!(
        timeout(Duration::from_secs(20), t.wait()).await.unwrap(),
        WriteReport::Unknown {
            accepted: 0,
            cause: WriteFailure::Io
        }
    ));
    assert!(r.client.is_closed());
    r.close().await;
}
