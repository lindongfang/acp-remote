#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use support::*;
use tokio::time::Instant;

#[tokio::test]
async fn ra06_wrong_frame_precedes_expired_cell_and_write_deadline_returns_ownership() {
    let r = Rig::ready(Version::V2).await;
    let res = r
        .client
        .reserve_request(
            r.prompt(),
            ResponseDeadline::At(Instant::now() + Duration::from_millis(100)),
        )
        .unwrap();
    let frame = res.encode().unwrap();
    let probe = res.probe();
    let other = r.reserve_prompt();
    let other_probe = other.probe();
    let wrong = other.encode().unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let error = r
        .client
        .enqueue(res, wrong, Arc::new(Fence), deadline())
        .unwrap_err();
    assert_eq!(error.error(), AcpError::InvalidFrame);
    let (_, res, wrong) = error.into_parts();
    drop(wrong);
    assert!(matches!(other_probe.progress(), WriteProgress::Reserved));
    let error = r
        .client
        .enqueue(res, frame, Arc::new(Fence), deadline())
        .unwrap_err();
    assert_eq!(error.error(), AcpError::Deadline);
    drop(error);
    assert!(matches!(
        probe.progress(),
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Deadline
        })
    ));
    drop(other);
    let res = r.reserve_prompt();
    let frame = res.encode().unwrap();
    let probe = res.probe();
    let error = r
        .client
        .enqueue(res, frame, Arc::new(Fence), Instant::now())
        .unwrap_err();
    assert_eq!(error.error(), AcpError::Deadline);
    drop(error);
    assert!(matches!(
        probe.progress(),
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Deadline
        })
    ));
    assert!(r.write.bytes().is_empty());
    r.close().await;
}

#[tokio::test]
async fn ra01_client_reserve_drop_releases_64_without_id_reuse() {
    let r = Rig::ready(Version::V2).await;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..128 {
        let res = r.reserve_prompt();
        assert!(seen.insert(rpc_value(res.id().unwrap()).to_string()));
        let p = res.probe();
        drop(res);
        assert!(matches!(
            p.progress(),
            WriteProgress::Finished(WriteReport::Zero {
                reason: ZeroReason::Abandoned
            })
        ));
        assert_eq!(r.client.pending_count(), 0);
    }
    let mut reserved = vec![];
    for _ in 0..64 {
        reserved.push(r.reserve_prompt());
    }
    assert_eq!(r.client.pending_count(), 64);
    assert!(matches!(
        r.client
            .reserve_request(r.prompt(), ResponseDeadline::Unbounded),
        Err(AcpError::Capacity)
    ));
    assert!(!r.client.is_closed());
    drop(reserved.pop());
    assert_eq!(r.client.pending_count(), 63);
    drop(r.reserve_prompt());
    drop(reserved);
    assert!(r.write.bytes().is_empty());
    r.close().await;
}
#[tokio::test]
async fn ra02_same_session_setup_cancel_releases_both_kinds() {
    let r = Rig::ready(Version::V2).await;
    let strategy = r.client.strategy().unwrap();
    let cfg = setup(Version::V2, "C:/synthetic");
    let first = r
        .client
        .reserve_request(
            strategy.resume(sid("A"), &cfg).unwrap(),
            ResponseDeadline::Unbounded,
        )
        .unwrap();
    assert!(
        r.client
            .reserve_request(
                strategy.replay(sid("A"), &cfg).unwrap(),
                ResponseDeadline::Unbounded
            )
            .is_err()
    );
    let probe = first.cancel();
    assert!(matches!(
        probe.progress(),
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Cancelled
        })
    ));
    let second = r
        .client
        .reserve_request(
            strategy.replay(sid("A"), &cfg).unwrap(),
            ResponseDeadline::Unbounded,
        )
        .unwrap();
    drop(second);
    drop(
        r.client
            .reserve_request(
                strategy.resume(sid("A"), &cfg).unwrap(),
                ResponseDeadline::Unbounded,
            )
            .unwrap(),
    );
    r.close().await;
}
#[tokio::test]
async fn ra04_ra06_enqueue_returns_objects_and_does_not_touch_wrong_cell() {
    let a = Rig::ready(Version::V2).await;
    let b = Rig::ready(Version::V2).await;
    let first = a.reserve_prompt();
    let first_frame = first.encode().unwrap();
    let first_probe = first.probe();
    let duplicate_frame = first.encode().unwrap();
    assert_eq!(duplicate_frame.bytes(), first_frame.bytes());
    drop(duplicate_frame);
    assert!(matches!(first_probe.progress(), WriteProgress::Reserved));
    assert_eq!(a.client.pending_count(), 1);
    let second = a.reserve_prompt();
    let second_probe = second.probe();
    let second_frame = second.encode().unwrap();
    let err = b
        .client
        .enqueue(first, second_frame, Arc::new(Fence), deadline())
        .unwrap_err();
    assert_eq!(err.error(), AcpError::InvalidRequestScope);
    let (_, first, second_frame) = err.into_parts();
    let err = a
        .client
        .enqueue(first, second_frame, Arc::new(Fence), deadline())
        .unwrap_err();
    assert_eq!(err.error(), AcpError::InvalidFrame);
    let (_, first, second_frame) = err.into_parts();
    assert!(matches!(first_probe.progress(), WriteProgress::Reserved));
    assert!(matches!(second_probe.progress(), WriteProgress::Reserved));
    assert!(a.write.bytes().is_empty());
    drop(second_frame);
    assert!(matches!(second_probe.progress(), WriteProgress::Reserved));
    drop(second);
    let ticket = a
        .client
        .enqueue(first, first_frame, Arc::new(Fence), deadline())
        .unwrap();
    finish(&ticket).await;
    assert_eq!(lf_count(a.write.bytes()), 1);
    a.close().await;
    b.close().await;
}
#[tokio::test]
async fn ra04_error_drop_only_abandons_its_reservation() {
    let r = Rig::ready(Version::V2).await;
    let a = r.reserve_prompt();
    let ap = a.probe();
    let b = r.reserve_prompt();
    let bp = b.probe();
    let bf = b.encode().unwrap();
    drop(
        r.client
            .enqueue(a, bf, Arc::new(Fence), deadline())
            .unwrap_err(),
    );
    assert!(matches!(
        ap.progress(),
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Abandoned
        })
    ));
    assert!(matches!(bp.progress(), WriteProgress::Reserved));
    assert_eq!(r.client.pending_count(), 1);
    drop(b);
    r.close().await;
}
#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep all four abandonment routes for the same once-only initialization invariant together"
)]
async fn ra07_initialize_abandoned_is_once_expired_before_reserve_is_not() {
    let r = Rig::spawn();
    assert!(matches!(
        r.client.reserve_initialize(Version::V2, Instant::now()),
        Err(AcpError::Deadline)
    ));
    let res = r
        .client
        .reserve_initialize(Version::V2, deadline())
        .unwrap();
    drop(res);
    assert!(matches!(
        r.client.reserve_initialize(Version::V2, deadline()),
        Err(AcpError::AlreadyInitialized)
    ));
    r.close().await;
    assert!(matches!(
        r.client.reserve_initialize(Version::V2, deadline()),
        Err(AcpError::AlreadyInitialized)
    ));
    for mode in 0..3 {
        let r = Rig::spawn();
        let response_at = if mode == 1 {
            Instant::now() + Duration::from_millis(100)
        } else {
            deadline()
        };
        let res = r
            .client
            .reserve_initialize(Version::V2, response_at)
            .unwrap();
        let probe = res.probe();
        match mode {
            0 => {
                drop(res.cancel());
                assert!(matches!(
                    probe.progress(),
                    WriteProgress::Finished(WriteReport::Zero {
                        reason: ZeroReason::Cancelled
                    })
                ));
            }
            1 => {
                tokio::time::sleep(Duration::from_millis(200)).await;
                assert!(matches!(
                    probe.progress(),
                    WriteProgress::Finished(WriteReport::Zero {
                        reason: ZeroReason::Deadline
                    })
                ));
                drop(res);
            }
            _ => {
                let other = Rig::spawn();
                let other_res = other
                    .client
                    .reserve_initialize(Version::V2, deadline())
                    .unwrap();
                let frame = other_res.encode().unwrap();
                let error = r
                    .client
                    .enqueue(res, frame, Arc::new(Fence), deadline())
                    .unwrap_err();
                assert_eq!(error.error(), AcpError::InvalidFrame);
                drop(error);
                drop(other_res);
                other.close().await;
            }
        }
        assert!(matches!(
            r.client.reserve_initialize(Version::V2, deadline()),
            Err(AcpError::AlreadyInitialized)
        ));
        assert!(r.write.bytes().is_empty());
        r.close().await;
    }
}
#[tokio::test]
async fn io06_pending_zero_cancel_then_next_request_and_no_late_write() {
    let r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
    let baseline = r.write.polls();
    let t = send(&r.client, r.reserve_prompt());
    r.write.wait_for(|w| w.polls() > baseline).await;
    assert!(matches!(
        t.cancel_and_wait().await,
        WriteReport::Zero {
            reason: ZeroReason::Cancelled
        }
    ));
    assert_eq!(r.client.pending_count(), 0);
    r.write.mode(WriteMode::Open);
    let second = send(&r.client, r.reserve_prompt());
    finish(&second).await;
    assert_eq!(lf_count(r.write.bytes()), 1);
    r.close().await;
}
#[tokio::test]
async fn io07_partial_cancel_poison_preserves_exact_accepted() {
    let r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Limit(3));
    let t = send(&r.client, r.reserve_prompt());
    r.write.wait_for(|w| w.bytes().len() == 3).await;
    assert!(matches!(
        t.cancel_and_wait().await,
        WriteReport::Unknown {
            accepted: 3,
            cause: WriteFailure::Cancelled
        }
    ));
    assert!(r.client.is_closed());
    r.write.mode(WriteMode::Open);
    r.close().await;
    assert_eq!(r.write.bytes().len(), 3);
    assert_eq!(r.write.shutdowns(), 0);
}
#[tokio::test]
async fn io08_response_before_flush_error_stays_received_once() {
    let mut r = Rig::ready(Version::V2).await;
    r.write.flush(WriteMode::Pending);
    let res = r.reserve_prompt();
    let id = res.id().unwrap().clone();
    let t = send(&r.client, res);
    r.write.wait_for(|w| w.bytes().ends_with(b"\n")).await;
    let received = r.respond(&id, json!({"messageId":"inserted"})).await;
    assert!(matches!(received.incoming(), Incoming::Response { .. }));
    assert_eq!(r.client.pending_count(), 0);
    r.write.flush(WriteMode::Fail);
    assert!(matches!(
        t.wait().await,
        WriteReport::Unknown {
            cause: WriteFailure::Io,
            ..
        }
    ));
    assert!(r.next().await.is_err());
    assert!(r.next().await.unwrap().is_none());
    r.close().await;
}
#[tokio::test]
async fn io09_reserved_guess_response_rejected_and_raw_retained() {
    let mut r = Rig::ready(Version::V2).await;
    let res = r.reserve_prompt();
    let id = res.id().unwrap().clone();
    let wire = serde_json::to_vec(
        &json!({"jsonrpc":"2.0","id":rpc_value(&id),"result":{"messageId":"forged"}}),
    )
    .unwrap();
    r.read.frame(&wire);
    assert!(r.next().await.is_err());
    assert_eq!(r.client.take_rejected_message().unwrap().bytes(), wire);
    assert!(r.client.is_closed());
    drop(res);
    r.close().await;
}
#[tokio::test]
async fn io15_reader_keeps_receiving_permission_under_writer_backpressure() {
    for v in [Version::V1, Version::V2] {
        let mut r = Rig::ready(v).await;
        r.write.mode(WriteMode::Pending);
        let baseline = r.write.polls();
        let t = send(&r.client, r.reserve_prompt());
        r.write.wait_for(|w| w.polls() > baseline).await;
        let id = RpcId::Integer(901);
        let wire = permission(v, &id);
        let received = r.feed(&wire).await;
        let Incoming::Permission(p) = received.incoming() else {
            panic!("permission not received")
        };
        assert_eq!(p.raw().bytes(), wire);
        assert_eq!(p.scope(), Some(r.client.scope()));
        assert_eq!(p.options().len(), 1);
        assert_eq!(p.options()[0].id().as_str(), "allow");
        assert_eq!(p.options()[0].kind(), "future_kind");
        assert!(!p.options()[0].supported());
        assert_eq!(r.client.server_pending_count(), 1);
        assert!(matches!(
            t.cancel_and_wait().await,
            WriteReport::Zero { .. }
        ));
        r.close().await;
    }
}
#[tokio::test]
async fn ra03_permission_reply_abandon_then_once_and_cross_scope_rejected() {
    let mut r = Rig::ready(Version::V1).await;
    let other = Rig::ready(Version::V1).await;
    let recv = r.feed(permission(Version::V1, &RpcId::Integer(9))).await;
    let Incoming::Permission(p) = recv.incoming() else {
        panic!()
    };
    assert!(
        other
            .client
            .reserve_permission_response(p, PermissionSelection::Selected(oid("allow")))
            .is_err()
    );
    let first = r
        .client
        .reserve_permission_response(p, PermissionSelection::Selected(oid("allow")))
        .unwrap();
    drop(first);
    let second = r
        .client
        .reserve_permission_response(p, PermissionSelection::Cancelled)
        .unwrap();
    drop(second.cancel());
    let third = r
        .client
        .reserve_permission_response(p, PermissionSelection::Selected(oid("allow")))
        .unwrap();
    let t = send(&r.client, third);
    finish(&t).await;
    assert!(
        r.client
            .reserve_permission_response(p, PermissionSelection::Cancelled)
            .is_err()
    );
    r.close().await;
    let next = send(&other.client, other.reserve_prompt());
    finish(&next).await;
    assert!(
        r.client
            .reserve_cancel(sid("A"), ExtensionFields::empty().unwrap())
            .is_err()
    );
    assert!(r.owner.shutdown(deadline()).await.joined);
    assert!(!other.client.is_closed());
    assert_eq!(other.client.pending_count(), 1);
    assert!(matches!(
        next.progress(),
        WriteProgress::Finished(WriteReport::Written)
    ));
    other.close().await;
}
#[tokio::test]
async fn dl01_reserved_deadline_advances_without_receiver_or_ticket_poll() {
    let r = Rig::ready(Version::V2).await;
    let res = r
        .client
        .reserve_request(
            r.prompt(),
            ResponseDeadline::At(Instant::now() + Duration::from_millis(100)),
        )
        .unwrap();
    let p = res.probe();
    // This is the response deadline oracle itself, not cleanup retry or timing-based race injection.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(matches!(
        p.progress(),
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Deadline
        })
    ));
    assert_eq!(r.client.pending_count(), 0);
    assert!(!r.client.is_closed());
    drop(res);
    assert!(r.write.bytes().is_empty());
    r.close().await;
}
#[tokio::test]
async fn io22_shutdown_concurrent_cancel_waiters_and_terminal_once() {
    let mut r = Rig::ready(Version::V2).await;
    r.write.mode(WriteMode::Pending);
    let t = send(&r.client, r.reserve_prompt());
    let (a, b) = tokio::join!(r.owner.shutdown(deadline()), r.owner.shutdown(deadline()));
    assert!(a.joined && b.joined);
    assert!(matches!(t.wait().await, WriteReport::Zero { .. }));
    let before = (r.write.polls(), r.read.polls());
    r.read.frame(notification("A"));
    r.write.mode(WriteMode::Open);
    assert!(r.next().await.unwrap().is_none());
    assert_eq!(before, (r.write.polls(), r.read.polls()));
    r.close().await;
}
#[test]
fn io01_no_runtime_spawn_is_typed_error() {
    let (_, reader, _) = ReadControl::new();
    let (_, writer, _) = WriteControl::new();
    assert!(matches!(
        Connection::spawn(reader, writer),
        Err(AcpError::RuntimeUnavailable)
    ));
}
