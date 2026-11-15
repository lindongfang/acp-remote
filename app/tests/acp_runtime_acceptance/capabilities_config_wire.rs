#[path = "support/mod.rs"]
mod support;
use acp_remote::adapters::acp::*;
use serde_json::json;
use std::sync::Arc;
use support::*;

async fn advertised(version: Version, result: serde_json::Value) -> Rig {
    let mut r = Rig::spawn();
    let res = r.client.reserve_initialize(version, deadline()).unwrap();
    let id = res.id().unwrap().clone();
    let t = send(&r.client, res);
    finish(&t).await;
    r.respond(&id, result).await;
    r.write.clear();
    r
}
#[tokio::test]
async fn cfg05_other_mcp_content_forwarded_v2_known_capabilities_still_checked() {
    let r=advertised(Version::V2,json!({"protocolVersion":2,"info":{"name":"independent","version":"synthetic"},"capabilities":{"session":{}}})).await;
    let s = r.client.strategy().unwrap();
    let other = McpServer::from_raw(
        Version::V2,
        owned(br#"{"type":"_future_transport","name":"future","secret":1e9999,"ordered":[3,2]}"#),
    )
    .unwrap();
    assert!(matches!(other.transport(),McpTransport::Other(t)if t.as_ref()=="_future_transport"));
    let snap = SetupSnapshot {
        wire: Version::V2,
        source: ConfigSourceStamp {
            reference: Arc::from("source"),
            revision: 7,
        },
        cwd: Known::Known(Arc::from("C:/synthetic")),
        mcp_servers: Known::Known(vec![other]),
        additional_directories: Known::Known(vec![]),
        extra: ExtensionFields::empty().unwrap(),
    };
    let setup = SessionSetup::try_from_snapshot(snap).unwrap();
    let req = s.new_session(&setup).unwrap();
    assert!(req.params().fragment().contains("1e9999"));
    let stdio = McpServer::stdio(
        Version::V2,
        Arc::from("stdio"),
        Arc::from("synthetic"),
        vec![],
        vec![],
        ExtensionFields::empty().unwrap(),
    )
    .unwrap();
    let missing = SessionSetup::try_from_snapshot(SetupSnapshot {
        wire: Version::V2,
        source: setup.source().clone(),
        cwd: Known::Known(Arc::from("C:/synthetic")),
        mcp_servers: Known::Known(vec![stdio]),
        additional_directories: Known::Known(vec![]),
        extra: ExtensionFields::empty().unwrap(),
    })
    .unwrap();
    assert!(s.new_session(&missing).is_err());
    let prompt = owned(br#"[{"type":"_future_content","number":1e9999,"escape":"\u0061"}]"#);
    let req = s
        .prompt(sid("A"), prompt, ExtensionFields::empty().unwrap())
        .unwrap();
    assert!(req.params().fragment().contains("1e9999"));
    assert!(req.params().fragment().contains("\\u0061"));
    assert!(
        s.prompt(
            sid("A"),
            owned(br#"[{"type":"image","data":"synthetic","mimeType":"image/png"}]"#),
            ExtensionFields::empty().unwrap()
        )
        .is_err()
    );
    r.close().await;
}
#[tokio::test]
async fn cfg07_reserved_decoded_keys_never_override_controlled_fields() {
    for version in [Version::V1, Version::V2] {
        let r = Rig::ready(version).await;
        let s = r.client.strategy().unwrap();
        for spelling in ["sessionId", "session\\u0049d", "prompt"] {
            let text = format!("{{\"{spelling}\":\"forged\"}}");
            match ExtensionFields::from_raw(owned(text)) {
                Err(e) => assert!(matches!(e, SetupError::ReservedField)),
                Ok(extra) => assert!(s.prompt(sid("A"), owned("[]"), extra).is_err()),
            }
        }
        assert!(ExtensionFields::from_raw(owned("[]")).is_err());
        let extra = ExtensionFields::from_raw(owned(
            br#"{ "_meta":null, "\u0065xtension": {"big":1e9999,"str":"\u0061"} }"#,
        ))
        .unwrap();
        let req = s.prompt(sid("A"), owned("[]"), extra).unwrap();
        let res = r
            .client
            .reserve_request(req, ResponseDeadline::Unbounded)
            .unwrap();
        let frame = res.encode().unwrap();
        let text = std::str::from_utf8(frame.bytes()).unwrap();
        assert!(text.contains("\\u0065xtension"));
        assert!(text.contains("1e9999"));
        assert!(text.contains("\\u0061"));
        assert_eq!(lf_count(frame.bytes()), 1);
        assert!(frame.bytes().ends_with(b"\n"));
        drop(res);
        assert!(r.write.bytes().is_empty());
        r.close().await;
    }
}
#[tokio::test]
async fn cf05_only_original_list_replay_listing_fields_keep_4096_not_new_resume() {
    for version in [Version::V1, Version::V2] {
        let r = Rig::ready(version).await;
        let s = r.client.strategy().unwrap();
        let cwd = "x".repeat(4097);
        let cfg = setup(version, &cwd);
        assert!(s.new_session(&cfg).is_ok());
        assert!(s.resume(sid("A"), &cfg).is_ok());
        assert!(matches!(s.replay(sid("A"), &cfg), Err(AcpError::Capacity)));
        assert!(matches!(
            s.list_sessions(Some(&cwd), None, ExtensionFields::empty().unwrap()),
            Err(AcpError::Capacity)
        ));
        assert!(matches!(
            s.list_sessions(None, Some(&cwd), ExtensionFields::empty().unwrap()),
            Err(AcpError::Capacity)
        ));
        let raw = raw_value(&json!({"sessions":[{"sessionId":"A","cwd":cwd}]}));
        assert!(matches!(
            s.session_listing(raw.as_ref()),
            Err(AcpError::Capacity)
        ));
        let raw = raw_value(&json!({"sessions":[],"nextCursor":"x".repeat(4097)}));
        assert!(matches!(
            s.session_listing(raw.as_ref()),
            Err(AcpError::Capacity)
        ));
        r.close().await;
    }
}
#[tokio::test]
async fn rt07_configured_replay_and_resume_encode_distinct_wire_without_loss() {
    for version in [Version::V1, Version::V2] {
        let r = Rig::ready(version).await;
        let s = r.client.strategy().unwrap();
        for replay in [false, true] {
            let cfg = setup(version, "C:/synthetic");
            let request = if replay {
                s.replay(sid("A"), &cfg)
            } else {
                s.resume(sid("A"), &cfg)
            }
            .unwrap();
            let res = r
                .client
                .reserve_request(request, ResponseDeadline::Unbounded)
                .unwrap();
            let frame = res.encode().unwrap();
            let wire: serde_json::Value = serde_json::from_slice(frame.bytes()).unwrap();
            assert_eq!(wire["params"]["cwd"], "C:/synthetic");
            assert_eq!(wire["params"]["mcpServers"], json!([]));
            assert_eq!(
                wire["method"],
                if replay && version == Version::V1 {
                    "session/load"
                } else {
                    "session/resume"
                }
            );
            if version == Version::V2 && replay {
                assert_eq!(wire["params"]["replayFrom"], json!({"type":"start"}));
            } else {
                assert!(wire["params"].get("replayFrom").is_none());
            }
            drop(res);
        }
        r.close().await;
    }
}
#[tokio::test]
async fn rt06_actual_selected_version_no_automatic_retry_on_business_error() {
    let mut r = advertised(Version::V2, handshake(Version::V1)).await;
    assert_eq!(r.client.strategy().unwrap().version(), Version::V1);
    assert_eq!(
        r.client.strategy().unwrap().negotiated().offered,
        Version::V2
    );
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
    let error=serde_json::to_vec(&json!({"jsonrpc":"2.0","id":rpc_value(&id),"error":{"code":-32000,"message":"SENSITIVE_REMOTE","data":{"number":3}}})).unwrap();
    let received = r.feed(&error).await;
    assert!(matches!(
        received.incoming(),
        Incoming::Response {
            response: Response::RemoteError { .. },
            ..
        }
    ));
    assert_eq!(received.raw().bytes(), error);
    assert!(!format!("{received:?}").contains("SENSITIVE_REMOTE"));
    assert_eq!(lf_count(r.write.bytes()), 1);
    assert!(matches!(
        r.client.reserve_initialize(Version::V1, deadline()),
        Err(AcpError::AlreadyInitialized)
    ));
    r.close().await;
}
