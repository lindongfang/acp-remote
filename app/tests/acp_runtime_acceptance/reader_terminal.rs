#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use std::time::Duration;
use support::*;
use tokio::time::Instant;

#[tokio::test]
async fn reader001_expiry_stop_wakes_writer_and_publishes_terminal_without_shutdown_help() {
    let mut r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Limit(3));
    let res = r
        .client
        .reserve_request(
            r.prompt(),
            ResponseDeadline::At(Instant::now() + Duration::from_secs(1)),
        )
        .unwrap();
    let t = send(&r.client, res);
    r.write.wait_for(|w| w.bytes().len() == 3).await;
    assert!(r.next().await.is_err()); // Only delivery consumer; no explicit shutdown or external write wake before terminal.
    assert!(matches!(
        t.wait().await,
        WriteReport::Unknown {
            accepted: 3,
            cause: WriteFailure::Deadline
        }
    ));
    assert!(r.next().await.unwrap().is_none());
    r.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reader002_read_error_or_eof_stops_before_later_writer_poll() {
    for fail in [false, true] {
        let mut r = Rig::ready(Version::V2).await;
        r.write.mode(WriteMode::Pending);
        let baseline = r.write.polls();
        let t = send(&r.client, r.reserve_prompt());
        r.write.wait_for(|w| w.polls() > baseline).await;
        r.read.terminal_and_open_writer(r.write.clone(), fail);
        let terminal = r.next().await;
        assert!(terminal.is_err() || matches!(terminal, Ok(None)));
        assert!(matches!(
            t.wait().await,
            WriteReport::Zero {
                reason: ZeroReason::ConnectionClosed
            }
        ));
        assert!(r.write.bytes().is_empty());
        assert_eq!(Some(r.write.polls()), r.read.terminal_writer_polls());
        r.close().await;
    }
}
