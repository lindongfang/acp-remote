#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};
use support::*;
use tokio::{
    sync::Notify,
    time::{Instant, timeout},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rc01_lf_read_before_first_write_never_gains_response_eligibility_after_parse() {
    let mut r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
    let baseline = r.write.polls();
    let res = r.reserve_prompt();
    let id = res.id().unwrap().clone();
    let t = send(&r.client, res);
    r.write.wait_for(|w| w.polls() > baseline).await;
    r.read.open_writer_at_next_lf(r.write.clone());
    let wire = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{{\"messageId\":\"too-early\"}},\"unknown\":[{}0]}}",
        rpc_value(&id),
        "null,".repeat(131_073)
    );
    r.read.frame(&wire);
    assert!(r.next().await.is_err());
    assert!(r.client.is_closed());
    assert_eq!(
        r.client.take_rejected_message().unwrap().bytes(),
        wire.as_bytes()
    );
    // Actual late accepted observation is separately recorded, never assumed from peer-buffer write.
    println!(
        "RC01 late_full_frame_observed={}",
        r.write.bytes().ends_with(b"\n")
    );
    let _ = t.wait().await;
    r.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rc02_state_lf_read_before_resume_write_keeps_live_origin() {
    let mut r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
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
    let t = send(&r.client, res);
    r.read.open_writer_at_next_lf(r.write.clone());
    let wire = format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\"sessionId\":\"A\",\"update\":{{\"sessionUpdate\":\"state_update\",\"state\":\"idle\",\"unknown\":[{}0]}}}}}}",
        "null,".repeat(131_073)
    );
    let received = r.feed(&wire).await;
    assert!(matches!(
        received.incoming(),
        Incoming::Update {
            origin: UpdateOrigin::Live,
            ..
        }
    ));
    println!(
        "RC02 late_window_request_fully_written={}",
        r.write.bytes().ends_with(b"\n")
    );
    r.close().await;
    let _ = t.wait().await;
}
struct Signal(Notify);
impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.0.notify_one();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.notify_one();
    }
}
#[tokio::test]
async fn dl10_owned_timer_wakes_registered_wait_without_consumer_polling() {
    let r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
    let baseline = r.write.polls();
    let res = r
        .client
        .reserve_request(
            r.prompt(),
            ResponseDeadline::At(Instant::now() + Duration::from_secs(1)),
        )
        .unwrap();
    let t = send(&r.client, res);
    r.write.wait_for(|w| w.polls() > baseline).await;
    let signal = Arc::new(Signal(Notify::new()));
    let waker = Waker::from(signal.clone());
    let mut cx = Context::from_waker(&waker);
    let mut wait = Box::pin(t.wait());
    assert!(matches!(
        Pin::as_mut(&mut wait).poll(&mut cx),
        Poll::Pending
    ));
    timeout(Duration::from_secs(20), signal.0.notified())
        .await
        .unwrap();
    assert!(matches!(
        t.progress(),
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Deadline
        })
    ));
    drop(wait);
    assert!(r.write.bytes().is_empty());
    r.close().await;
}
