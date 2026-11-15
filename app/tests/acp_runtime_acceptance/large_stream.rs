#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use support::*;

#[tokio::test]
async fn rt05_real_inbound_outbound_over_32mib_no_frame_ceiling() {
    for version in [Version::V1, Version::V2] {
        let mut r = Rig::ready(version).await;
        let text = "x".repeat(32 * 1024 * 1024 + 129);
        let prompt = raw_value(&json!([{"type":"text","text":text}]));
        let request = r
            .client
            .strategy()
            .unwrap()
            .prompt(sid("A"), prompt, ExtensionFields::empty().unwrap())
            .unwrap();
        let res = r
            .client
            .reserve_request(request, ResponseDeadline::Unbounded)
            .unwrap();
        let frame = res.encode().unwrap();
        assert!(frame.bytes().len() > 32 * 1024 * 1024);
        let expected = frame.bytes().to_vec();
        let t = r
            .client
            .enqueue(res, frame, std::sync::Arc::new(Fence), deadline())
            .unwrap();
        finish(&t).await;
        assert_eq!(r.write.bytes(), expected);
        let wire=serde_json::to_vec(&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"A","update":{"sessionUpdate":"future_display","unknown":text}}})).unwrap();
        assert!(wire.len() > 32 * 1024 * 1024);
        let received = r.feed(&wire).await;
        assert_eq!(received.raw().bytes(), wire);
        assert!(!r.client.is_closed());
        r.close().await;
    }
}
#[tokio::test]
async fn rt05_deep_and_many_tokens_actual_wire_clone_drop_and_compact() {
    for version in [Version::V1, Version::V2] {
        let mut r = Rig::ready(version).await;
        for subtree in [
            format!("{}1e99999{}", "[".repeat(20_000), "]".repeat(20_000)),
            format!("[{}0]", "null,".repeat(131_073)),
        ] {
            let fragment = format!(
                "{{\"future\":{subtree},\"escaped\\u005fkey\":\"\\u0061\\n\",\"number\":-0}}"
            );
            let ext = ExtensionFields::from_raw(owned(&fragment)).unwrap();
            let request = r
                .client
                .strategy()
                .unwrap()
                .prompt(sid("A"), owned("[]"), ext)
                .unwrap();
            let res = r
                .client
                .reserve_request(request, ResponseDeadline::Unbounded)
                .unwrap();
            let frame = res.encode().unwrap();
            let expected = frame.bytes().to_vec();
            assert!(std::str::from_utf8(&expected).unwrap().contains(&subtree));
            let t = r
                .client
                .enqueue(res, frame, std::sync::Arc::new(Fence), deadline())
                .unwrap();
            finish(&t).await;
            assert!(r.write.bytes().ends_with(&expected));
            let wire = format!(
                "  {{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\"sessionId\":\"A\",\"update\":{{\"sessionUpdate\":\"future_display\",\"opaque\":{fragment}}}}}}}  "
            );
            let received = r.feed(wire.as_bytes()).await;
            let copy = received.clone();
            drop(received);
            assert_eq!(copy.raw().bytes(), wire.as_bytes());
            drop(copy);
            assert!(!r.client.is_closed());
        }
        r.close().await;
    }
}
#[tokio::test]
async fn cf06_empty_prompt_is_really_sent_both_versions() {
    for version in [Version::V1, Version::V2] {
        let mut r = Rig::ready(version).await;
        let req = r
            .client
            .strategy()
            .unwrap()
            .prompt(sid("A"), owned("[]"), ExtensionFields::empty().unwrap())
            .unwrap();
        let res = r
            .client
            .reserve_request(req, ResponseDeadline::Unbounded)
            .unwrap();
        let id = res.id().unwrap().clone();
        let t = send(&r.client, res);
        finish(&t).await;
        let value: serde_json::Value = serde_json::from_slice(&r.write.bytes()).unwrap();
        assert_eq!(value["params"]["prompt"], json!([]));
        let result = if version == Version::V1 {
            json!({"stopReason":"end_turn"})
        } else {
            json!({"messageId":"inserted"})
        };
        assert!(matches!(
            r.respond(&id, result).await.incoming(),
            Incoming::Response { .. }
        ));
        assert_eq!(r.client.pending_count(), 0);
        r.close().await;
    }
}
#[tokio::test]
async fn io18_both_versions_8194_requests_keep_seen_forever() {
    for version in [Version::V1, Version::V2] {
        let mut r = Rig::ready(version).await;
        for n in 0..8194 {
            let id = RpcId::Integer(n);
            let wire = if n % 2 == 0 {
                permission(version, &id)
            } else {
                serde_json::to_vec(&json!({"jsonrpc":"2.0","id":n,"method":"future/request","params":{"unknown":n}})).unwrap()
            };
            let received = r.feed(wire).await;
            let res = match received.incoming() {
                Incoming::Permission(p) => r
                    .client
                    .reserve_permission_response(p, PermissionSelection::Cancelled)
                    .unwrap(),
                Incoming::UnsupportedRequest(p) => r.client.reserve_unsupported(p).unwrap(),
                _ => panic!("request not preserved"),
            };
            let t = send(&r.client, res);
            finish(&t).await;
            assert_eq!(r.client.server_pending_count(), 0);
            assert!(!r.client.is_closed());
        }
        r.read.frame(permission(version, &RpcId::Integer(0)));
        assert!(r.next().await.is_err());
        assert!(r.client.is_closed());
        r.close().await;
    }
}
