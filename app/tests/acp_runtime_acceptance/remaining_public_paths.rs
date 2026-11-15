#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use support::*;

#[tokio::test]
async fn rt04_foreign_strategy_request_cannot_reserve_or_change_capacity() {
    let a = Rig::ready(Version::V2).await;
    let b = Rig::ready(Version::V2).await;
    let request = a.prompt();
    assert!(matches!(
        b.client
            .reserve_request(request, ResponseDeadline::Unbounded),
        Err(AcpError::InvalidRequestScope)
    ));
    assert_eq!(a.client.pending_count(), 0);
    assert_eq!(b.client.pending_count(), 0);
    assert!(a.write.bytes().is_empty() && b.write.bytes().is_empty());
    a.close().await;
    b.close().await;
}
#[tokio::test]
async fn rt10_close_request_is_control_method_with_extra_preserved() {
    for version in [Version::V1, Version::V2] {
        let r = Rig::ready(version).await;
        let s = r.client.strategy().unwrap();
        let req = s
            .close(
                sid("A"),
                ExtensionFields::from_raw(owned("{\"unknown\":1e9999}")).unwrap(),
            )
            .unwrap();
        assert!(matches!(req.kind(),RequestKind::Close(session)if session.as_str()=="A"));
        let res = r
            .client
            .reserve_request(req, ResponseDeadline::Unbounded)
            .unwrap();
        let frame = res.encode().unwrap();
        let text = std::str::from_utf8(frame.bytes()).unwrap();
        assert!(text.contains("session/close"));
        assert!(!text.contains("session/cancel"));
        assert!(text.contains("1e9999"));
        assert!(text.contains("\"sessionId\":\"A\""));
        drop(res);
        r.close().await;
    }
}
#[tokio::test]
async fn rt08_prompt_shape_known_type_cap_and_v1_unknown_are_not_text_fallback() {
    for version in [Version::V1, Version::V2] {
        let r = Rig::ready(version).await;
        let s = r.client.strategy().unwrap();
        for raw in ["null", "{}", "7", "[{\"type\":\"text\"}]"] {
            assert!(
                s.prompt(sid("A"), owned(raw), ExtensionFields::empty().unwrap())
                    .is_err()
            );
        }
        if version == Version::V1 {
            assert!(
                s.prompt(
                    sid("A"),
                    owned("[{\"type\":\"_future\",\"opaque\":1}]"),
                    ExtensionFields::empty().unwrap()
                )
                .is_err()
            );
        }
        assert!(r.write.bytes().is_empty());
        r.close().await;
    }
}
#[tokio::test]
async fn cfg04_sse_v1_preserves_headers_and_never_changes_transport_to_http() {
    let r = Rig::ready(Version::V1).await;
    let server = McpServer::sse_v1(
        Arc::from("sse"),
        Arc::from("https://synthetic.invalid"),
        vec![
            NamedValue::new(
                Arc::from("X-Synthetic"),
                Arc::from("a"),
                ExtensionFields::empty().unwrap(),
            )
            .unwrap(),
        ],
        ExtensionFields::empty().unwrap(),
    )
    .unwrap();
    assert!(matches!(server.transport(), McpTransport::Sse));
    let snapshot = SetupSnapshot {
        wire: Version::V1,
        source: ConfigSourceStamp {
            reference: Arc::from("synthetic"),
            revision: 7,
        },
        cwd: Known::Known(Arc::from("C:/synthetic")),
        mcp_servers: Known::Known(vec![server]),
        additional_directories: Known::Known(vec![]),
        extra: ExtensionFields::empty().unwrap(),
    };
    let req = r
        .client
        .strategy()
        .unwrap()
        .new_session(&SessionSetup::try_from_snapshot(snapshot).unwrap())
        .unwrap();
    let o = req
        .params()
        .object()
        .unwrap()
        .get("mcpServers")
        .unwrap()
        .unwrap()
        .array()
        .unwrap()
        .get(0)
        .unwrap()
        .object()
        .unwrap();
    assert_eq!(o.get("type").unwrap().unwrap().string().unwrap(), "sse");
    assert_eq!(o.get("headers").unwrap().unwrap().array().unwrap().len(), 1);
    r.close().await;
}
#[tokio::test]
async fn cfg06_additional_roots_order_and_missing_advertisement_is_path_limited() {
    for version in [Version::V1, Version::V2] {
        let mut r = Rig::spawn();
        let res = r.client.reserve_initialize(version, deadline()).unwrap();
        let id = res.id().unwrap().clone();
        let t = send(&r.client, res);
        finish(&t).await;
        let result = if version == Version::V1 {
            json!({"protocolVersion":1,"agentCapabilities":{}})
        } else {
            json!({"protocolVersion":2,"info":{"name":"independent","version":"synthetic"},"capabilities":{"session":{}}})
        };
        r.respond(&id, result).await;
        r.write.clear();
        let cfg = SessionSetup::try_from_snapshot(SetupSnapshot {
            wire: version,
            source: ConfigSourceStamp {
                reference: Arc::from("source"),
                revision: 7,
            },
            cwd: Known::Known(Arc::from("C:/synthetic")),
            mcp_servers: Known::Known(vec![]),
            additional_directories: Known::Known(vec![Arc::from("C:/a"), Arc::from("C:/b")]),
            extra: ExtensionFields::empty().unwrap(),
        })
        .unwrap();
        assert_eq!(
            cfg.additional_directories()
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<&str>>(),
            ["C:/a", "C:/b"]
        );
        let strategy = r.client.strategy().unwrap();
        assert!(strategy.new_session(&cfg).is_err());
        assert!(
            strategy
                .prompt(sid("A"), owned("[]"), ExtensionFields::empty().unwrap())
                .is_ok()
        );
        r.close().await;
    }
}
#[tokio::test]
async fn io09_partial_or_pending_zero_cannot_accept_guessed_response() {
    for partial in [false, true] {
        let mut r = Rig::ready(Version::V2).await;
        r.write.mode(if partial {
            WriteMode::Limit(3)
        } else {
            WriteMode::Pending
        });
        let baseline = r.write.polls();
        let res = r.reserve_prompt();
        let id = res.id().unwrap().clone();
        let t = send(&r.client, res);
        r.write.wait_for(|w| w.polls() > baseline).await;
        let wire = serde_json::to_vec(
            &json!({"jsonrpc":"2.0","id":rpc_value(&id),"result":{"messageId":"too-early"}}),
        )
        .unwrap();
        r.read.frame(&wire);
        assert!(r.next().await.is_err());
        assert_eq!(r.client.take_rejected_message().unwrap().bytes(), wire);
        assert!(r.client.is_closed());
        let _ = t.wait().await;
        r.close().await;
    }
}
#[tokio::test]
async fn rc05_rc06_stop_on_first_frame_drains_all_already_read_lf_frames() {
    let mut r = Rig::ready(Version::V2).await;
    let res = r.reserve_prompt();
    let id = res.id().unwrap().clone();
    let t = send(&r.client, res);
    finish(&t).await;
    let invalid = json!({"jsonrpc":"2.0","id":"unmatched","result":{}});
    let valid = json!({"jsonrpc":"2.0","id":rpc_value(&id),"result":{"messageId":"real"}});
    let update = notification("A");
    let mut batch = format!("{invalid}\n{valid}\n").into_bytes();
    batch.extend_from_slice(&update);
    batch.push(b'\n');
    batch.extend_from_slice(&permission(Version::V2, &RpcId::Integer(700)));
    batch.push(b'\n');
    batch
        .extend_from_slice(br#"{"jsonrpc":"2.0","id":701,"method":"_future/request","params":{}}"#);
    batch.push(b'\n');
    batch.extend_from_slice(b"{\"partial\":");
    r.read.push(batch);
    let response = r.next().await.unwrap().unwrap();
    assert!(matches!(response.incoming(),Incoming::Response{id:actual,..}if *actual==id));
    let event = r.next().await.unwrap().unwrap();
    assert_eq!(event.raw().bytes(), update);
    assert_eq!(response.receipt_order(), event.receipt_order());
    let p = r.next().await.unwrap().unwrap();
    let Incoming::Permission(p) = p.incoming() else {
        panic!()
    };
    assert!(matches!(
        r.client
            .reserve_permission_response(p, PermissionSelection::Cancelled),
        Err(AcpError::Closed)
    ));
    let u = r.next().await.unwrap().unwrap();
    let Incoming::UnsupportedRequest(u) = u.incoming() else {
        panic!()
    };
    assert!(matches!(
        r.client.reserve_unsupported(u),
        Err(AcpError::Closed)
    ));
    assert_eq!(r.client.server_pending_count(), 0);
    assert!(r.next().await.is_err());
    assert!(r.next().await.unwrap().is_none());
    assert_eq!(
        r.client
            .take_rejected_message()
            .unwrap()
            .field("id")
            .unwrap()
            .unwrap()
            .string()
            .unwrap(),
        "unmatched"
    );
    r.close().await;
}
#[tokio::test]
async fn io17_canceled_zero_cell_does_not_resume_after_control_succeeds() {
    let mut r = Rig::ready(Version::V1).await;
    r.write.mode(WriteMode::Pending);
    let baseline = r.write.polls();
    let normal = send(&r.client, r.reserve_prompt());
    r.write.wait_for(|w| w.polls() > baseline).await;
    let request = r.feed(permission(Version::V1, &RpcId::Integer(50))).await;
    let Incoming::Permission(p) = request.incoming() else {
        panic!()
    };
    let control = send(
        &r.client,
        r.client
            .reserve_permission_response(p, PermissionSelection::Cancelled)
            .unwrap(),
    );
    normal.request_cancel();
    r.write.mode(WriteMode::Open);
    finish(&control).await;
    assert!(matches!(
        normal.wait().await,
        WriteReport::Zero {
            reason: ZeroReason::Cancelled
        }
    ));
    let bytes = r.write.bytes();
    let frames = bytes
        .split(|&b| b == b'\n')
        .filter(|b| !b.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 1);
    let wire: serde_json::Value = serde_json::from_slice(frames[0]).unwrap();
    assert_eq!(wire["id"], 50);
    r.close().await;
}
#[tokio::test]
async fn io20_partial_reply_is_spent_and_cannot_be_reserved_again() {
    let mut r = Rig::ready(Version::V1).await;
    let received = r.feed(permission(Version::V1, &RpcId::Integer(70))).await;
    let Incoming::Permission(p) = received.incoming() else {
        panic!()
    };
    r.write.mode(WriteMode::Limit(3));
    let t = send(
        &r.client,
        r.client
            .reserve_permission_response(p, PermissionSelection::Cancelled)
            .unwrap(),
    );
    r.write.wait_for(|w| w.bytes().len() == 3).await;
    assert!(matches!(
        t.cancel_and_wait().await,
        WriteReport::Unknown { accepted: 3, .. }
    ));
    assert!(
        r.client
            .reserve_permission_response(p, PermissionSelection::Selected(oid("allow")))
            .is_err()
    );
    r.close().await;
    assert_eq!(r.write.bytes().len(), 3);
}
#[tokio::test]
async fn dl09_v1_unbounded_prompt_written_is_not_cut_off_at_two_seconds() {
    let mut r = Rig::ready(Version::V1).await;
    let res = r.reserve_prompt();
    let id = res.id().unwrap().clone();
    let t = send(&r.client, res);
    finish(&t).await;
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert!(!r.client.is_closed());
    assert!(matches!(
        t.progress(),
        WriteProgress::Finished(WriteReport::Written)
    ));
    assert!(matches!(
        r.respond(&id, json!({"stopReason":"end_turn"}))
            .await
            .incoming(),
        Incoming::Response {
            response: Response::Prompt(PromptResult::Completed { .. }),
            ..
        }
    ));
    r.close().await;
}
#[tokio::test]
async fn io24_shutdown_resolves_current_partial_and_full_unflushed_tickets() {
    for full in [false, true] {
        let r = Rig::ready(Version::V2).await;
        if full {
            r.write.flush(WriteMode::Pending);
        } else {
            r.write.mode(WriteMode::Limit(3));
        }
        let t = send(&r.client, r.reserve_prompt());
        r.write
            .wait_for(|w| {
                if full {
                    w.bytes().ends_with(b"\n")
                } else {
                    w.bytes().len() == 3
                }
            })
            .await;
        assert!(r.owner.shutdown(deadline()).await.joined);
        assert!(matches!(t.wait().await, WriteReport::Unknown { .. }));
        r.close().await;
    }
}
