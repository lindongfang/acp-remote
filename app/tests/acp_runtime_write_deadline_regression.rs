//! Independent write-deadline regression; real scripted IO, real received brands.
#[path = "acp_runtime_acceptance/support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use std::sync::Arc;
use support::*;
use tokio::time::{Duration, Instant, timeout};

async fn permission(rig: &mut Rig, version: Version) -> PermissionRequest {
    let mut params = json!({"sessionId":"A","options":[{"optionId":"allow","name":"synthetic","kind":"allow_once"}]});
    if version == Version::V1 {
        params["toolCall"] = json!({"toolCallId":"own-tool"});
    } else {
        params["title"] = json!("synthetic");
    }
    let received = rig.feed(serde_json::to_vec(&json!({"jsonrpc":"2.0","id":"deadline-permission","method":"session/request_permission","params":params})).unwrap()).await;
    let Incoming::Permission(request) = received.incoming() else {
        panic!("real permission")
    };
    request.clone()
}
fn enqueue(rig: &Rig, reservation: Reservation, at: Instant) -> WriteTicket {
    let frame = reservation.encode().unwrap();
    rig.client
        .enqueue(reservation, frame, Arc::new(Fence), at)
        .unwrap()
}
async fn result(ticket: &WriteTicket) -> WriteReport {
    timeout(Duration::from_secs(5), ticket.wait())
        .await
        .unwrap()
}

#[tokio::test]
async fn wd01_permission_pending0_expires_without_consumers_and_restores_waiting() {
    for version in [Version::V1, Version::V2] {
        let mut rig = Rig::ready(version).await;
        let request = permission(&mut rig, version).await;
        rig.write.mode(WriteMode::Pending);
        let polls = rig.write.polls();
        let reservation = rig
            .client
            .reserve_permission_response(&request, PermissionSelection::Cancelled)
            .unwrap();
        let ticket = enqueue(
            &rig,
            reservation,
            Instant::now() + Duration::from_millis(50),
        );
        rig.write.wait_for(|w| w.polls() > polls).await;
        assert!(rig.write.bytes().is_empty());
        let probe = ticket.probe();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let observer = std::thread::spawn(move || {
            // Actual Pending was signalled. This observes the absolute deadline; it does not infer IO readiness.
            std::thread::sleep(Duration::from_millis(200));
            tx.send(probe.progress()).unwrap();
        });
        assert_eq!(
            rx.await.unwrap(),
            WriteProgress::Finished(WriteReport::Zero {
                reason: ZeroReason::Deadline
            })
        );
        observer.join().unwrap();
        assert_eq!(
            result(&ticket).await,
            WriteReport::Zero {
                reason: ZeroReason::Deadline
            }
        );
        assert!(!rig.client.is_closed());
        assert!(rig.write.bytes().is_empty());
        rig.write.mode(WriteMode::Open);
        let reservation = rig
            .client
            .reserve_permission_response(&request, PermissionSelection::Cancelled)
            .unwrap();
        let explicit = enqueue(&rig, reservation, deadline());
        assert_eq!(result(&explicit).await, WriteReport::Written);
        assert!(rig.write.bytes().ends_with(b"\n"));
        assert_eq!(
            std::str::from_utf8(&rig.write.bytes())
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert!(
            rig.client
                .reserve_permission_response(&request, PermissionSelection::Cancelled)
                .is_err()
        );
        rig.close().await;
    }
}

#[tokio::test]
async fn wd02_partial_reply_deadline_is_unknown_and_never_resumes() {
    for version in [Version::V1, Version::V2] {
        let mut rig = Rig::ready(version).await;
        let request = permission(&mut rig, version).await;
        rig.write.mode(WriteMode::Limit(8));
        let polls = rig.write.polls();
        let reservation = rig
            .client
            .reserve_permission_response(&request, PermissionSelection::Cancelled)
            .unwrap();
        let ticket = enqueue(
            &rig,
            reservation,
            Instant::now() + Duration::from_millis(50),
        );
        rig.write
            .wait_for(|w| w.bytes().len() == 8 && w.polls() >= polls + 2)
            .await;
        assert_eq!(
            result(&ticket).await,
            WriteReport::Unknown {
                accepted: 8,
                cause: WriteFailure::Deadline
            }
        );
        assert!(rig.client.is_closed());
        let polls = rig.write.polls();
        rig.write.mode(WriteMode::Open);
        assert!(
            rig.client
                .reserve_permission_response(&request, PermissionSelection::Cancelled)
                .is_err()
        );
        rig.close().await;
        assert_eq!(rig.write.polls(), polls);
        assert_eq!(rig.write.bytes().len(), 8);
        assert!(!rig.write.bytes().ends_with(b"\n"));
    }
}

#[tokio::test]
async fn wd03_full_frame_flush_deadline_keeps_early_received_and_real_unknown() {
    for version in [Version::V1, Version::V2] {
        for reply in [true, false] {
            let mut rig = Rig::ready(version).await;
            let request = if reply {
                Some(permission(&mut rig, version).await)
            } else {
                None
            };
            rig.write.flush(WriteMode::Pending);
            let flushes = rig.write.flushes();
            let reservation = if let Some(request) = &request {
                rig.client
                    .reserve_permission_response(request, PermissionSelection::Cancelled)
                    .unwrap()
            } else {
                let request = rig
                    .client
                    .strategy()
                    .unwrap()
                    .list_sessions(None, None, ExtensionFields::empty().unwrap())
                    .unwrap();
                rig.client
                    .reserve_request(request, ResponseDeadline::Unbounded)
                    .unwrap()
            };
            let id = reservation.id().unwrap().clone();
            let ticket = enqueue(
                &rig,
                reservation,
                Instant::now() + Duration::from_millis(100),
            );
            rig.write.wait_for(|w| w.flushes() > flushes).await;
            let total = rig.write.bytes().len();
            assert!(rig.write.bytes().ends_with(b"\n"));
            let early = if reply {
                None
            } else {
                let received = rig
                    .respond(&id, json!({"sessions":[],"future":"own-raw"}))
                    .await;
                assert!(matches!(
                    received.incoming(),
                    Incoming::Response {
                        response: Response::Sessions(_),
                        ..
                    }
                ));
                Some(received)
            };
            assert_eq!(
                result(&ticket).await,
                WriteReport::Unknown {
                    accepted: total,
                    cause: WriteFailure::Deadline
                }
            );
            assert!(rig.client.is_closed());
            if let Some(received) = early {
                assert!(matches!(
                    received.incoming(),
                    Incoming::Response {
                        response: Response::Sessions(_),
                        ..
                    }
                ));
                assert!(
                    received
                        .raw()
                        .bytes()
                        .windows(b"own-raw".len())
                        .any(|s| s == b"own-raw")
                );
            }
            let flushes = rig.write.flushes();
            rig.write.flush(WriteMode::Open);
            rig.close().await;
            assert_eq!(rig.write.flushes(), flushes);
            assert_eq!(rig.write.bytes().len(), total);
            assert_eq!(
                result(&ticket).await,
                WriteReport::Unknown {
                    accepted: total,
                    cause: WriteFailure::Deadline
                }
            );
        }
    }
}

#[tokio::test]
async fn wd04_written_cancel_and_stop_keep_first_terminal_and_join() {
    for branch in 0..3 {
        let mut rig = Rig::ready(Version::V2).await;
        let request = permission(&mut rig, Version::V2).await;
        if branch != 0 {
            rig.write.mode(WriteMode::Pending);
        }
        let polls = rig.write.polls();
        let at = Instant::now()
            + if branch == 0 {
                Duration::from_millis(50)
            } else {
                Duration::from_secs(2)
            };
        let reservation = rig
            .client
            .reserve_permission_response(&request, PermissionSelection::Cancelled)
            .unwrap();
        let ticket = enqueue(&rig, reservation, at);
        let expected = match branch {
            0 => {
                assert_eq!(result(&ticket).await, WriteReport::Written);
                tokio::time::sleep_until(at + Duration::from_millis(20)).await;
                assert!(!rig.client.is_closed());
                WriteReport::Written
            }
            1 => {
                rig.write.wait_for(|w| w.polls() > polls).await;
                assert_eq!(
                    ticket.cancel_and_wait().await,
                    WriteReport::Zero {
                        reason: ZeroReason::Cancelled
                    }
                );
                assert!(!rig.client.is_closed());
                WriteReport::Zero {
                    reason: ZeroReason::Cancelled,
                }
            }
            _ => {
                rig.write.wait_for(|w| w.polls() > polls).await;
                rig.close().await;
                WriteReport::Zero {
                    reason: ZeroReason::ConnectionClosed,
                }
            }
        };
        assert_eq!(result(&ticket).await, expected);
        rig.close().await;
        assert_eq!(result(&ticket).await, expected);
    }
}
