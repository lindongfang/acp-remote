#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use std::{future::Future, pin::Pin, task::Poll, time::Duration};
use support::*;
use tokio::time::timeout;

#[tokio::test]
async fn io23_ticket_drop_without_ambient_runtime_keeps_owned_cancel() {
    let r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
    let baseline = r.write.polls();
    let ticket = send(&r.client, r.reserve_prompt());
    r.write.wait_for(|w| w.polls() > baseline).await;
    let probe = ticket.probe();
    std::thread::spawn(move || drop(ticket)).join().unwrap();
    r.close().await;
    assert!(matches!(
        probe.progress(),
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Abandoned
        })
    ));
    assert!(r.write.bytes().is_empty());
}
#[tokio::test]
async fn io23_receiver_drop_without_ambient_runtime_can_still_join() {
    let r = Rig::ready(Version::V2).await;
    let Rig {
        client,
        receiver,
        owner,
        read: _,
        write,
        read_weak,
        write_weak,
    } = r;
    std::thread::spawn(move || drop(receiver)).join().unwrap();
    assert!(client.is_closed());
    assert!(owner.shutdown(deadline()).await.joined);
    assert!(read_weak.upgrade().is_none());
    assert!(write_weak.upgrade().is_none());
    assert!(write.bytes().is_empty());
}

async fn first_poll<F: Future>(mut f: Pin<&mut F>) -> Poll<F::Output> {
    std::future::poll_fn(|cx| Poll::Ready(f.as_mut().poll(cx))).await
}

#[tokio::test]
async fn io05_unpolled_ticket_cancel_and_drop_never_first_write() {
    for abandon in [false, true] {
        let r = Rig::ready(Version::V2).await;
        let polls = r.write.polls();
        let res = r.reserve_prompt();
        let frame = res.encode().unwrap();
        let t = r
            .client
            .enqueue(res, frame, std::sync::Arc::new(Fence), deadline())
            .unwrap();
        let p = t.probe();
        if abandon {
            drop(t);
        } else {
            assert!(matches!(
                t.cancel_and_wait().await,
                WriteReport::Zero {
                    reason: ZeroReason::Cancelled
                }
            ));
        }
        r.close().await;
        assert_eq!(r.write.polls(), polls);
        assert!(
            matches!(p.progress(),WriteProgress::Finished(WriteReport::Zero{reason})if reason==if abandon{ZeroReason::Abandoned}else{ZeroReason::Cancelled})
        );
    }
}
#[tokio::test]
async fn ra08_borrowed_wait_drop_keeps_ticket_and_cancel_wait_drop_keeps_command() {
    let r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
    let t = send(&r.client, r.reserve_prompt());
    let mut waiting = Box::pin(t.wait());
    assert!(first_poll(waiting.as_mut()).await.is_pending());
    drop(waiting);
    r.write.mode(WriteMode::Open);
    finish(&t).await;
    r.close().await;
    let r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
    let t = send(&r.client, r.reserve_prompt());
    let mut waiting = Box::pin(t.cancel_and_wait());
    let _ = first_poll(waiting.as_mut()).await;
    drop(waiting);
    assert!(matches!(
        t.wait().await,
        WriteReport::Zero {
            reason: ZeroReason::Cancelled
        }
    ));
    r.close().await;
}
#[tokio::test]
async fn io22_actual_pending_shutdown_waiter_drop_preserves_join_handles() {
    let r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
    let t = send(&r.client, r.reserve_prompt());
    let mut waiting = Box::pin(r.owner.shutdown(deadline()));
    assert!(first_poll(waiting.as_mut()).await.is_pending());
    drop(waiting);
    assert!(r.owner.shutdown(deadline()).await.joined);
    assert!(matches!(t.wait().await, WriteReport::Zero { .. }));
    r.close().await;
}
#[tokio::test]
async fn io22_past_shutdown_deadline_is_not_join_ack_and_later_close_can_join() {
    let r = Rig::ready(Version::V2).await;
    let report = r.owner.shutdown(tokio::time::Instant::now()).await;
    if !report.joined {
        assert!(r.owner.shutdown(deadline()).await.joined);
    }
    r.close().await;
}
#[tokio::test]
async fn io23_owner_drop_from_no_runtime_thread_stops_and_reclaims_io() {
    let r = Rig::ready(Version::V2).await;
    let Rig {
        client,
        receiver,
        owner,
        read,
        write,
        read_weak,
        write_weak,
    } = r;
    let reads = read.polls();
    let writes = write.polls();
    std::thread::spawn(move || drop(owner)).join().unwrap();
    timeout(Duration::from_secs(20), async {
        while read_weak.upgrade().is_some() || write_weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(client.is_closed());
    assert!(
        client
            .reserve_cancel(sid("A"), ExtensionFields::empty().unwrap())
            .is_err()
    );
    assert_eq!(read.polls(), reads);
    assert_eq!(write.polls(), writes);
    drop(receiver);
    drop(client);
}
#[tokio::test]
async fn rc08_spawn_immediate_eof_or_read_error_publishes_terminal_then_real_join() {
    for fail in [false, true] {
        let (read, r, rweak) = ReadControl::new();
        let (_, w, wweak) = WriteControl::new();
        if fail {
            read.fail();
        } else {
            read.eof();
        }
        let (client, mut receiver, owner) = Connection::spawn(r, w).unwrap();
        let result = timeout(Duration::from_secs(20), receiver.next())
            .await
            .unwrap();
        assert!(result.is_err() || matches!(result, Ok(None)));
        assert!(owner.shutdown(deadline()).await.joined);
        assert!(client.is_closed());
        assert!(rweak.upgrade().is_none());
        assert!(wweak.upgrade().is_none());
        assert!(receiver.next().await.unwrap().is_none());
    }
}
#[tokio::test]
async fn rc08_spawn_immediate_writer_error_keeps_both_tasks_owned() {
    let (_read, r, rweak) = ReadControl::new();
    let (write, w, wweak) = WriteControl::new();
    write.mode(WriteMode::Fail);
    let (client, mut receiver, owner) = Connection::spawn(r, w).unwrap();
    let res = client.reserve_initialize(Version::V2, deadline()).unwrap();
    let t = send(&client, res);
    assert!(matches!(
        t.wait().await,
        WriteReport::Unknown {
            accepted: 0,
            cause: WriteFailure::Io
        }
    ));
    assert!(receiver.next().await.is_err());
    assert!(owner.shutdown(deadline()).await.joined);
    assert!(rweak.upgrade().is_none());
    assert!(wweak.upgrade().is_none());
    assert!(client.is_closed());
}
#[tokio::test]
async fn io26_exact_crlf_whitespace_split_bytes_eof_and_half_frame() {
    let mut r = Rig::ready(Version::V2).await;
    let raw=b"  {\"jsonrpc\":\"2.0\",\"method\":\"future/notification\",\"params\":{\"escaped\":\"\\u0061\"}}  ";
    let mut frame = raw.to_vec();
    frame.extend_from_slice(b"\r\n");
    for b in frame {
        r.read.push(vec![b]);
    }
    let received = r.next().await.unwrap().unwrap();
    assert_eq!(received.raw().bytes(), raw);
    let one = notification("A");
    let mut batch = one.clone();
    batch.push(b'\n');
    batch.extend_from_slice(&one);
    batch.push(b'\n');
    batch.extend_from_slice(b"{\"incomplete\":");
    r.read.push(batch);
    r.read.eof();
    let a = r.next().await.unwrap().unwrap();
    let b = r.next().await.unwrap().unwrap();
    assert_eq!(a.raw().bytes(), one);
    assert_eq!(b.raw().bytes(), one);
    assert!(a.sequence() < b.sequence());
    assert!(r.next().await.is_err());
    assert!(r.next().await.unwrap().is_none());
    r.close().await;
}
