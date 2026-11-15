#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::Context,
};
use support::*;

#[tokio::test]
async fn rt13_pure_permission_has_no_brand_unknown_option_cannot_consume_live_request() {
    for version in [Version::V1, Version::V2] {
        let mut r = Rig::ready(version).await;
        let wire = permission(version, &RpcId::Integer(7));
        let raw = RawMessage::parse(wire.clone()).unwrap();
        let strategy = r.client.strategy().unwrap();
        let parsed = strategy
            .permission(
                RpcId::Integer(7),
                raw.clone(),
                raw.field("params").unwrap().unwrap(),
            )
            .unwrap();
        assert!(parsed.scope().is_none());
        assert!(
            r.client
                .reserve_permission_response(&parsed, PermissionSelection::Selected(oid("allow")))
                .is_err()
        );
        let received = r.feed(&wire).await;
        let Incoming::Permission(p) = received.incoming() else {
            panic!()
        };
        assert!(
            r.client
                .reserve_permission_response(
                    p,
                    PermissionSelection::Selected(oid("not-advertised"))
                )
                .is_err()
        );
        assert_eq!(r.client.server_pending_count(), 1);
        assert!(r.write.bytes().is_empty());
        let t = send(
            &r.client,
            r.client
                .reserve_permission_response(p, PermissionSelection::Selected(oid("allow")))
                .unwrap(),
        );
        finish(&t).await;
        assert_eq!(r.client.server_pending_count(), 0);
        assert!(
            r.client
                .reserve_permission_response(p, PermissionSelection::Cancelled)
                .is_err()
        );
        r.close().await;
        let mut duplicate = Rig::ready(version).await;
        let mut value: serde_json::Value =
            serde_json::from_slice(&permission(version, &RpcId::Integer(8))).unwrap();
        let option = value["params"]["options"][0].clone();
        value["params"]["options"] = json!([option.clone(), option]);
        duplicate.read.frame(serde_json::to_vec(&value).unwrap());
        match duplicate.next().await {
            Ok(Some(event)) => {
                let Incoming::Permission(request) = event.incoming() else {
                    panic!("ambiguous permission must reject request or selection")
                };
                assert!(
                    duplicate
                        .client
                        .reserve_permission_response(
                            request,
                            PermissionSelection::Selected(oid("allow"))
                        )
                        .is_err()
                );
            }
            Err(_) => assert!(duplicate.client.is_closed()),
            _ => panic!("ambiguous permission must not disappear"),
        }
        assert!(duplicate.write.bytes().is_empty());
        duplicate.close().await;
    }
}
#[tokio::test]
async fn ra03_unsupported_reply_drop_cancel_and_foreign_scope() {
    let mut r = Rig::ready(Version::V2).await;
    let foreign = Rig::ready(Version::V2).await;
    let wire=br#"{"jsonrpc":"2.0","id":"unsupported","method":"_future/request","params":{"unknown":1e9999}}"#;
    let received = r.feed(wire).await;
    let Incoming::UnsupportedRequest(p) = received.incoming() else {
        panic!()
    };
    assert_eq!(p.id(), &RpcId::String(Arc::from("unsupported")));
    assert_eq!(p.method(), "_future/request");
    assert_eq!(p.scope(), r.client.scope());
    assert_eq!(p.raw().bytes(), wire);
    assert!(foreign.client.reserve_unsupported(p).is_err());
    drop(r.client.reserve_unsupported(p).unwrap());
    let probe = r.client.reserve_unsupported(p).unwrap().cancel();
    assert!(matches!(
        probe.progress(),
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Cancelled
        })
    ));
    let t = send(&r.client, r.client.reserve_unsupported(p).unwrap());
    finish(&t).await;
    assert!(r.client.reserve_unsupported(p).is_err());
    r.close().await;
    foreign.close().await;
}
#[tokio::test]
async fn io04_two_directions_64_independent_and_65_server_poison() {
    let mut r = Rig::ready(Version::V2).await;
    let mut client = vec![];
    for _ in 0..64 {
        client.push(r.reserve_prompt());
    }
    let mut requests = vec![];
    for id in 0..64 {
        requests.push(
            r.feed(
                serde_json::to_vec(
                    &json!({"jsonrpc":"2.0","id":id,"method":"_future/request","params":{}}),
                )
                .unwrap(),
            )
            .await,
        );
    }
    assert_eq!(r.client.pending_count(), 64);
    assert_eq!(r.client.server_pending_count(), 64);
    assert!(!r.client.is_closed());
    assert!(matches!(
        r.client
            .reserve_request(r.prompt(), ResponseDeadline::Unbounded),
        Err(AcpError::Capacity)
    ));
    assert!(!r.client.is_closed());
    r.read
        .frame(br#"{"jsonrpc":"2.0","id":64,"method":"_future/request","params":{}}"#);
    assert!(r.next().await.is_err());
    assert!(r.client.is_closed());
    drop(client);
    drop(requests);
    r.close().await;
}
#[tokio::test]
async fn io18_typed_same_spelling_server_ids_reply_independently() {
    let mut rig = Rig::ready(Version::V1).await;
    let a = rig.feed(permission(Version::V1, &RpcId::Integer(1))).await;
    let b = rig
        .feed(permission(Version::V1, &RpcId::String(Arc::from("1"))))
        .await;
    assert_eq!(rig.client.server_pending_count(), 2);
    for received in [&a, &b] {
        let Incoming::Permission(request) = received.incoming() else {
            panic!()
        };
        let t = send(
            &rig.client,
            rig.client
                .reserve_permission_response(request, PermissionSelection::Cancelled)
                .unwrap(),
        );
        finish(&t).await;
    }
    assert_eq!(rig.client.server_pending_count(), 0);
    let bytes = rig.write.bytes();
    let lines = bytes
        .split(|&b| b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(lines[0]["id"], 1);
    assert_eq!(lines[1]["id"], "1");
    rig.close().await;
}
struct RevisionFence {
    scope: ConnectionScope,
    current: AtomicU64,
}
impl WriteFence for RevisionFence {
    fn check_and_poll(
        &self,
        id: &WriteIdentity,
        _: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        assert_eq!(id.scope(), self.scope);
        assert_eq!(id.setup_stamp().unwrap().revision, 7);
        assert!(matches!(
            id.kind(),
            OutboundKind::Request(RequestKind::NewSession)
        ));
        assert!(id.id().is_some());
        if self.current.load(Ordering::SeqCst) != 7 {
            return Err(FenceRejection::RevisionChanged);
        }
        let _ = io.poll(cx);
        Ok(())
    }
}
#[tokio::test]
async fn cfg09_revision_checked_again_after_pending_zero_before_real_write() {
    let r = Rig::ready(Version::V2).await;
    let req = r
        .client
        .strategy()
        .unwrap()
        .new_session(&setup(Version::V2, "C:/synthetic"))
        .unwrap();
    let res = r
        .client
        .reserve_request(req, ResponseDeadline::Unbounded)
        .unwrap();
    assert_eq!(res.scope(), r.client.scope());
    assert_eq!(res.setup_stamp().unwrap().revision, 7);
    assert!(res.id().is_some());
    assert!(matches!(
        res.kind(),
        OutboundKind::Request(RequestKind::NewSession)
    ));
    let probe = res.probe();
    assert_eq!(probe.scope(), res.scope());
    assert_eq!(probe.id(), res.id());
    let frame = res.encode().unwrap();
    let fence = Arc::new(RevisionFence {
        scope: r.client.scope(),
        current: AtomicU64::new(7),
    });
    r.write.mode(WriteMode::Pending);
    let baseline = r.write.polls();
    let ticket = r
        .client
        .enqueue(res, frame, fence.clone(), deadline())
        .unwrap();
    r.write.wait_for(|w| w.polls() > baseline).await;
    fence.current.store(8, Ordering::SeqCst);
    r.write.mode(WriteMode::Open);
    assert!(matches!(
        ticket.wait().await,
        WriteReport::Zero {
            reason: ZeroReason::FenceRejected
        }
    ));
    assert!(r.write.bytes().is_empty());
    r.close().await;
}
#[tokio::test]
async fn rt17_every_public_data_debug_is_sensitive_text_free() {
    let mut r = Rig::ready(Version::V2).await;
    let secret = "SENSITIVE_MARKER_PRIVATE";
    let raw = owned(format!("{{\"unknown\":\"{secret}\"}}"));
    let ext = ExtensionFields::from_raw(raw.clone()).unwrap();
    let req = r
        .client
        .strategy()
        .unwrap()
        .prompt(sid("A"), owned("[]"), ext.clone())
        .unwrap();
    let object = OrderedRawObject::from_raw(raw.clone()).unwrap();
    let meta = RawMetadata::from_raw(raw).unwrap();
    let res = r
        .client
        .reserve_request(req, ResponseDeadline::Unbounded)
        .unwrap();
    let frame = res.encode().unwrap();
    let probe = res.probe();
    for debug in [
        format!("{ext:?}"),
        format!("{object:?}"),
        format!("{meta:?}"),
        format!("{res:?}"),
        format!("{frame:?}"),
        format!("{probe:?}"),
    ] {
        assert!(!debug.contains(secret));
    }
    drop(frame);
    drop(res);
    let wire = format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"{secret}\",\"params\":{{\"text\":\"{secret}\"}}}}"
    );
    let received = r.feed(wire.as_bytes()).await;
    assert!(!format!("{received:?}").contains(secret));
    assert!(!format!("{:?}", received.incoming()).contains(secret));
    assert!(!format!("{:?}", received.raw()).contains(secret));
    r.close().await;
}
